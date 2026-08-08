#!/usr/bin/env python3
"""Run a terminal-only, three-model CP7 Rally panel from verified StoreV2 checkpoints.

This is the pair-void-tolerant sibling of run_cp7_store_panel_v1.py (which stays
untouched). It adds one capability: when --tolerate-engine-faults is passed, a
deterministic XMage engine critical-error fault (never a bridge I/O failure or a
candidate-side assertion; see XMageRallyAnchorSpike.isEngineCriticalFault on the
Java side) voids the whole pair it occurred in instead of failing the entire
campaign. Both legs of a voided pair contribute nothing to outcomes. Voided
pairs are still fully schema-validated as evidence (not discarded as noise),
recorded per task and per model in the panel summary, and capped: if more than
2% of a model's requested pairs are voided, the run fails anyway.

Everything else (Store binding, outcome-jsonl validation, terminal win/draw/loss
aggregation) is unchanged from v1. With --tolerate-engine-faults unset, this
script's behavior is identical to v1's all-or-nothing semantics.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any


# Two model authority modes, matching checkpoint_shadow_stdio_v1's two
# ShadowCheckpointAuthorityV1 loading paths. "population" issues
# --population-store-root (PopulationStoreGeneration): any store trained
# under the environment-randomization-v2 contract, generation required.
# "original" issues --store-root with an explicit --generation
# (OriginalPromoted2StoreGeneration): the promoted(2) lineage pin, hard-wired
# to accept only the one canonical promoted(2) run (its own source_run_sha256
# constant, verified Rust-side) and to the legacy-v1 environment contract.
# The two authority_kind / environment_trajectory_contract wire strings below
# are read verbatim from checkpoint_shadow_stdio_v1's own source
# (native_checkpoint_shadow_stdio_v1.rs: AUTHORITY_KIND matches
# PopulationStoreGeneration's "population-store-validated-generation" /
# OriginalPromoted2StoreGeneration's "original-promoted2-validated-store-
# generation"; ENVIRONMENT_CONTRACT matches POPULATION_STORE_ENVIRONMENT_
# TRAJECTORY_CONTRACT_V1 / SOURCE_ENVIRONMENT_TRAJECTORY_CONTRACT_V1) so the
# outcome-jsonl "checkpoint" field's exact-match validation in
# validate_outcome_shard agrees with what the binary actually emits.
MODEL_AUTHORITY_MODES = ("population", "original", "fixedstate")
AUTHORITY_KIND = "population-store-validated-generation"
AUTHORITY_KIND_ORIGINAL = "original-promoted2-validated-store-generation"
ENVIRONMENT_CONTRACT = "environment-randomization-v2"
ENVIRONMENT_CONTRACT_ORIGINAL = "legacy-v1"

# Third authority mode: checkpoint_shadow_stdio_v1's XmageCp7OutcomeDerivative
# "fixed native state" sub-path (--xmage-cp7-outcome-root PATH, no --generation).
# Unlike population/original, this does not read a population-Store layout at
# all: PATH is a staging directory containing exactly two files, a canonical
# fixed_native_state.json manifest and a checkpoint.state.f32le payload copy
# (see stage_fixed_native_state.py-style staging, done once, out of band).
# Every field name/value below is read verbatim from
# native_checkpoint_shadow_stdio_v1.rs's XmageCp7OutcomeDerivative dispatch
# (checkpoint_shadow_stdio_v1, mtg-kernel-composed-factorial-v1-codex,
# read-only reference): the six FIXED_NATIVE_STATE_SOURCE_* constants below are
# what that dispatch unconditionally reports as source_run_sha256 /
# source_generation / source_checkpoint_sha256 / source_sidecar_sha256 /
# source_payload_sha256 / source_train_state_sha256 / loaded_run_sha256 for
# EVERY fixed-native-state load, regardless of which state was actually
# loaded -- they are the promoted(2) g384 anchor's own identity (independently
# cross-checked here: they match promoted(2)'s own run.json-embedded
# opponent_ladder_initialization block byte for byte).
FIXED_NATIVE_STATE_SCHEMA = "mtg-kernel-xmage-fixed-native-state/v1"
FIXED_NATIVE_STATE_PAYLOAD_FILENAME = "checkpoint.state.f32le"
FIXED_NATIVE_STATE_MANIFEST_FILENAME = "fixed_native_state.json"
FIXED_NATIVE_STATE_NON_CLAIMS = [
    "external software anchor is not professional-level evidence",
    "terminal win/loss/draw is the only playing-strength outcome",
]
FIXED_NATIVE_STATE_SOURCE_RUN_SHA256 = \
    "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae"
FIXED_NATIVE_STATE_SOURCE_GENERATION = 384
FIXED_NATIVE_STATE_SOURCE_CHECKPOINT_SHA256 = \
    "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8"
FIXED_NATIVE_STATE_SOURCE_SIDECAR_SHA256 = \
    "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb"
FIXED_NATIVE_STATE_SOURCE_PAYLOAD_SHA256 = \
    "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99"
FIXED_NATIVE_STATE_SOURCE_TRAIN_STATE_SHA256 = \
    "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8"
FIXED_NATIVE_STATE_ENVIRONMENT_CONTRACT = "environment-randomization-v2"
SAMPLER_IDENTITY = "f32-q8-expq63-hamilton-splitmix64-v1"
SAMPLER_CONTRACT = "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6"
OUTCOME_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
# checkpoint_shadow_stdio_v1's outcome-jsonl writer emits schema v1 instead of
# v2 whenever a loaded checkpoint's identity matches the promoted(2) g384
# anchor exactly, field for field (exact_g384, native_checkpoint_shadow_
# stdio_v1.rs) -- which is exactly what --store-root --generation 384
# (original mode) always resolves to in this harness. v1 rows are v2's rows
# minus the per-row "checkpoint" block (confirmed field-for-field against a
# real promoted2 outcome export): the header still carries checkpoint once,
# but decision/terminal rows do not repeat it, since v1 has no other
# checkpoint it could ever be. Schema v1 is therefore accepted ONLY for
# models bound via original mode; population and fixedstate models must
# still carry the full v2 per-row checkpoint block, strictly, with no v1
# fallback -- see validate_outcome_shard.
OUTCOME_CONTRACT_V1 = "mtg-kernel-xmage-cp7-outcome-jsonl/v1"
CARD_DB_HASH = "b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d"
HEX_16 = re.compile(r"[0-9a-f]{16}\Z")
HEX_32 = re.compile(r"[0-9a-f]{32}\Z")
HEX_64 = re.compile(r"[0-9a-f]{64}\Z")
DECISION_KEYS = {
    "record_type", "schema_version", "record_ordinal", "outcome_decision_ordinal",
    "pair_index", "episode_id", "candidate_seat", "base_seed_u64_hex",
    "pair_environment_seed_u64_hex", "deck_ids", "environment_revision",
    "randomization_identity", "selection_source", "acting_player", "step",
    "decision_kind", "physical_decision_id", "actor_physical_decision_ordinal",
    "substep_index", "substep_count", "legal_action_count", "selected_index",
    "selected_semantic", "candidate_order_commitment_128_hex", "action_semantics",
    "tensor", "model_input_sha256", "old_policy_logits_f32_bits",
    "old_value_f32_bits", "checkpoint",
}
DECISION_KEYS_V1 = DECISION_KEYS - {"checkpoint"}
TERMINAL_KEYS = {
    "record_type", "schema_version", "record_ordinal", "pair_index", "episode_id",
    "candidate_seat", "base_seed_u64_hex", "pair_environment_seed_u64_hex",
    "deck_ids", "randomization_identity", "core_environment_hash_u64_hex",
    "diagnostic_state_hash_u64_hex", "first_outcome_decision_ordinal",
    "outcome_decision_count", "terminal", "candidate_terminal_reward", "checkpoint",
}
TERMINAL_KEYS_V1 = TERMINAL_KEYS - {"checkpoint"}

# Structured stdout markers emitted by XMageRallyAnchorSpike (mage-kernel-anchor-
# spike-v1). PAIR_VOID_MARKER/SPIKE_VOID_STOP_MARKER only ever appear when the
# harness was launched with --tolerate-engine-faults / AI_ANCHOR_TOLERATE_ENGINE_
# FAULTS=1; SPIKE_PASS_MARKER is the unchanged all-pairs-completed summary.
PAIR_VOID_MARKER = "XMAGE_RALLY_ANCHOR_PAIR_VOID"
SPIKE_VOID_STOP_MARKER = "XMAGE_RALLY_ANCHOR_SPIKE VOID_STOP"
SPIKE_PASS_MARKER = "XMAGE_RALLY_ANCHOR_SPIKE PASS"
VOID_CAP_FRACTION_NUMERATOR = 2
VOID_CAP_FRACTION_DENOMINATOR = 100


def fail(message: str) -> None:
    raise ValueError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json_bytes(value: Any, *, indent: int | None = None) -> bytes:
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True,
                       indent=indent, separators=None if indent is not None else (",", ":"))
            + "\n").encode("utf-8")


def exclusive_write(path: Path, payload: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise


def load_canonical_json(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    if not raw.endswith(b"\n") or b"\r" in raw:
        fail(f"noncanonical JSON file: {path}")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"invalid JSON {path}: {error}")
    if not isinstance(value, dict):
        fail(f"JSON document is not an object: {path}")
    return value


def require_sha(value: Any, context: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or any(
        character not in "0123456789abcdef" for character in value
    ):
        fail(f"invalid SHA-256 {context}")
    return value


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, check=True,
    )
    return completed.stdout.splitlines()[0] if completed.stdout else ""


def _git_commit(repo: Path) -> str:
    return _version(["git", "-c", f"safe.directory={repo}", "-C", str(repo),
                     "rev-parse", "HEAD"])


def load_fixed_native_state_identity(root: Path, generation: int) -> dict[str, Any]:
    if generation < 0 or not root.is_dir():
        fail("fixed native state root or generation is invalid")
    root = root.resolve()
    manifest_path = root / FIXED_NATIVE_STATE_MANIFEST_FILENAME
    payload_path = root / FIXED_NATIVE_STATE_PAYLOAD_FILENAME
    entries = sorted(entry.name for entry in root.iterdir() if entry.is_file())
    if entries != sorted((FIXED_NATIVE_STATE_MANIFEST_FILENAME, FIXED_NATIVE_STATE_PAYLOAD_FILENAME)):
        fail(f"fixed native state directory must contain exactly the manifest and payload: {root}")
    manifest = load_canonical_json(manifest_path)
    if (
        manifest.get("schema") != FIXED_NATIVE_STATE_SCHEMA
        or not isinstance(manifest.get("authority_kind"), str) or not manifest["authority_kind"]
        or manifest.get("non_claims") != FIXED_NATIVE_STATE_NON_CLAIMS
    ):
        fail(f"fixed native state manifest schema, authority_kind, or non_claims mismatch: {manifest_path}")
    payload = manifest.get("payload")
    if not isinstance(payload, dict) or payload.get("filename") != FIXED_NATIVE_STATE_PAYLOAD_FILENAME:
        fail(f"fixed native state payload block is invalid: {manifest_path}")
    require_sha(manifest.get("source_result_sha256"), "fixed native state source result")
    payload_sha = require_sha(payload.get("payload_sha256"), "fixed native state payload")
    require_sha(payload.get("parameters_sha256"), "fixed native state parameters")
    require_sha(payload.get("first_moments_sha256"), "fixed native state first moments")
    require_sha(payload.get("second_moments_sha256"), "fixed native state second moments")
    model_parameter_sha = require_sha(payload.get("model_parameter_sha256"),
                                       "fixed native state model parameter")
    native_state_sha = require_sha(payload.get("native_state_sha256"), "fixed native state native state")
    if not _plain_int(payload.get("adam_step")) or payload["adam_step"] != generation:
        fail(f"fixed native state adam_step does not match the requested generation: {manifest_path}")
    if not _plain_int(payload.get("byte_count")) or payload["byte_count"] != payload_path.stat().st_size:
        fail(f"fixed native state payload byte_count mismatch: {payload_path}")
    if sha256(payload_path) != payload_sha:
        fail(f"fixed native state payload sha256 mismatch: {payload_path}")
    manifest_sha = sha256(manifest_path)
    identity = {
        "authority_kind": manifest["authority_kind"],
        "source_run_sha256": FIXED_NATIVE_STATE_SOURCE_RUN_SHA256,
        "source_generation": FIXED_NATIVE_STATE_SOURCE_GENERATION,
        "source_checkpoint_sha256": FIXED_NATIVE_STATE_SOURCE_CHECKPOINT_SHA256,
        "source_sidecar_sha256": FIXED_NATIVE_STATE_SOURCE_SIDECAR_SHA256,
        "source_payload_sha256": FIXED_NATIVE_STATE_SOURCE_PAYLOAD_SHA256,
        "source_train_state_sha256": FIXED_NATIVE_STATE_SOURCE_TRAIN_STATE_SHA256,
        "loaded_run_sha256": FIXED_NATIVE_STATE_SOURCE_RUN_SHA256,
        "loaded_generation": generation,
        "loaded_checkpoint_sha256": manifest_sha,
        "loaded_payload_sha256": payload_sha,
        "loaded_train_state_sha256": native_state_sha,
        "model_parameter_sha256": model_parameter_sha,
        "environment_trajectory_contract": FIXED_NATIVE_STATE_ENVIRONMENT_CONTRACT,
        "sampler_identity": SAMPLER_IDENTITY,
        "sampler_contract_sha256": SAMPLER_CONTRACT,
    }
    return {
        "root": str(root), "generation": generation, "mode": "fixedstate", "checkpoint": identity,
        "store_files": {
            "manifest_sha256": manifest_sha, "payload_sha256": payload_sha,
            "parameters_sha256": payload["parameters_sha256"],
            "first_moments_sha256": payload["first_moments_sha256"],
            "second_moments_sha256": payload["second_moments_sha256"],
        },
        "payload": {
            "byte_count": payload["byte_count"],
            "parameters_sha256": payload["parameters_sha256"],
            "first_moments_sha256": payload["first_moments_sha256"],
            "second_moments_sha256": payload["second_moments_sha256"],
        },
    }


def load_store_identity(root: Path, generation: int, *,
                        mode: str = "population") -> dict[str, Any]:
    if mode not in MODEL_AUTHORITY_MODES:
        fail(f"invalid model authority mode: {mode}")
    if mode == "fixedstate":
        return load_fixed_native_state_identity(root, generation)
    if generation < 0 or not root.is_dir():
        fail("population Store root or generation is invalid")
    root = root.resolve()
    stem = f"update-{generation:08d}"
    run_path = root / "run.json"
    checkpoint_path = root / "checkpoints" / f"{stem}.checkpoint.json"
    sidecar_path = root / "checkpoints" / f"{stem}.sidecar.json"
    state_path = root / "checkpoints" / f"{stem}.state.f32le"
    latest_path = root / "latest.json"
    if not all(path.is_file() for path in (run_path, checkpoint_path, sidecar_path, state_path, latest_path)):
        fail(f"population Store generation files are incomplete: {root} g{generation}")
    run = load_canonical_json(run_path)
    checkpoint = load_canonical_json(checkpoint_path)
    sidecar = load_canonical_json(sidecar_path)
    latest = load_canonical_json(latest_path)
    run_sha = sha256(run_path)
    checkpoint_sha = sha256(checkpoint_path)
    sidecar_sha = sha256(sidecar_path)
    state_sha = sha256(state_path)
    if (
        run.get("schema") != "mtg_kernel_native_train_run/v2"
        or run.get("store_identity") != "mtg-kernel-native-training-store-v2"
        or checkpoint.get("schema") != "mtg_kernel_native_train_checkpoint/v3"
        or sidecar.get("schema") != "mtg_kernel_native_train_checkpoint_sidecar/v2"
        or latest.get("schema") != "mtg_kernel_native_train_latest/v2"
        or checkpoint.get("generation_index") != generation
        or sidecar.get("generation_index") != generation
        or not _plain_int(latest.get("generation_index"))
        or latest["generation_index"] < generation
    ):
        fail("population Store schema or generation mismatch")
    payload = checkpoint.get("payload")
    train_state = checkpoint.get("train_state")
    contracts = run.get("contracts")
    sampler = contracts.get("learner_sampler") if isinstance(contracts, dict) else None
    if not isinstance(payload, dict) or not isinstance(train_state, dict) or not isinstance(sampler, dict):
        fail("population Store checkpoint fields are incomplete")
    if (
        payload.get("byte_count") != state_path.stat().st_size
        or require_sha(payload.get("sha256"), "checkpoint payload") != state_sha
        or require_sha(checkpoint.get("run_sha256"), "checkpoint run") != run_sha
        or require_sha(sidecar.get("run_sha256"), "sidecar run") != run_sha
        or require_sha(sidecar.get("checkpoint_manifest_sha256"), "sidecar checkpoint") != checkpoint_sha
        or require_sha(sidecar.get("checkpoint_payload_sha256"), "sidecar payload") != state_sha
        or require_sha(sidecar.get("train_state_sha256"), "sidecar train state")
            != require_sha(train_state.get("state_sha256"), "checkpoint train state")
        or require_sha(sidecar.get("model_parameter_sha256"), "sidecar model")
            != require_sha(train_state.get("model_parameter_sha256"), "checkpoint model")
        or require_sha(latest.get("run_sha256"), "latest run") != run_sha
        or require_sha(latest.get("identity_bundle_sha256"), "latest identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or require_sha(run.get("contracts", {}).get("identity_bundle_sha256"), "run identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or sampler.get("identity") != SAMPLER_IDENTITY
        or sampler.get("contract_sha256") != SAMPLER_CONTRACT
    ):
        fail("population Store checkpoint chain mismatch")
    authority_kind = AUTHORITY_KIND if mode == "population" else AUTHORITY_KIND_ORIGINAL
    environment_contract = (
        ENVIRONMENT_CONTRACT if mode == "population" else ENVIRONMENT_CONTRACT_ORIGINAL)
    identity = {
        "authority_kind": authority_kind,
        "source_run_sha256": run_sha,
        "source_generation": generation,
        "source_checkpoint_sha256": checkpoint_sha,
        "source_sidecar_sha256": sidecar_sha,
        "source_payload_sha256": state_sha,
        "source_train_state_sha256": train_state["state_sha256"],
        "loaded_run_sha256": run_sha,
        "loaded_generation": generation,
        "loaded_checkpoint_sha256": checkpoint_sha,
        "loaded_payload_sha256": state_sha,
        "loaded_train_state_sha256": train_state["state_sha256"],
        "model_parameter_sha256": train_state["model_parameter_sha256"],
        "environment_trajectory_contract": environment_contract,
        "sampler_identity": SAMPLER_IDENTITY,
        "sampler_contract_sha256": SAMPLER_CONTRACT,
    }
    return {
        "root": str(root), "generation": generation, "mode": mode, "checkpoint": identity,
        "store_files": {
            "run_json_sha256": run_sha, "checkpoint_json_sha256": checkpoint_sha,
            "sidecar_json_sha256": sidecar_sha, "state_payload_sha256": state_sha,
            "identity_bundle_sha256": checkpoint["identity_bundle_sha256"],
            "store_head_generation": latest["generation_index"],
        },
        "payload": {
            "byte_count": payload["byte_count"],
            "parameters_sha256": payload["sections"][0]["sha256"],
            "first_moments_sha256": payload["sections"][1]["sha256"],
            "second_moments_sha256": payload["sections"][2]["sha256"],
        },
    }


def maven_opts(identity: dict[str, Any]) -> str:
    prefix = "-Dxmage.rally.populationStore."
    names = {
        "authorityKind": "authority_kind", "sourceRunSha256": "source_run_sha256",
        "sourceGeneration": "source_generation", "sourceCheckpointSha256": "source_checkpoint_sha256",
        "sourceSidecarSha256": "source_sidecar_sha256", "sourcePayloadSha256": "source_payload_sha256",
        "sourceTrainStateSha256": "source_train_state_sha256", "loadedRunSha256": "loaded_run_sha256",
        "loadedGeneration": "loaded_generation", "loadedCheckpointSha256": "loaded_checkpoint_sha256",
        "loadedPayloadSha256": "loaded_payload_sha256", "loadedTrainStateSha256": "loaded_train_state_sha256",
        "modelParameterSha256": "model_parameter_sha256",
        "environmentTrajectoryContract": "environment_trajectory_contract",
        "samplerIdentity": "sampler_identity", "samplerContractSha256": "sampler_contract_sha256",
    }
    return " ".join(prefix + key + "=" + str(identity[value]) for key, value in names.items())


def xmage_cp7_outcome_maven_opts(identity: dict[str, Any]) -> str:
    # Names and required set verified against XMageCp7OutcomeExpectation.
    # fromSystemProperties() (XMageRallyBridgeProcessClient.java): a smaller
    # set than population-store's, since source_run_sha256 and friends are
    # always the fixed promoted(2) anchor for this authority and Java does
    # not ask Python to assert them separately.
    prefix = "-Dxmage.rally.cp7Outcome."
    names = {
        "authorityKind": "authority_kind", "adamStep": "loaded_generation",
        "manifestSha256": "loaded_checkpoint_sha256", "payloadSha256": "loaded_payload_sha256",
        "trainStateSha256": "loaded_train_state_sha256",
        "modelParameterSha256": "model_parameter_sha256",
        "environmentTrajectoryContract": "environment_trajectory_contract",
    }
    return " ".join(prefix + key + "=" + str(identity[value]) for key, value in names.items())


def chunk_ranges(pair_start: int, pairs: int, task_pairs: int) -> list[tuple[int, int]]:
    if pair_start < 0 or pairs < 1 or task_pairs < 1:
        fail("invalid shard range")
    stop = pair_start + pairs
    return [(first, min(task_pairs, stop - first))
            for first in range(pair_start, stop, task_pairs)]


def planned_tasks(labels: list[str], chunks: list[tuple[int, int]]) -> list[dict[str, Any]]:
    return [
        {"label": label, "first_pair": first_pair, "pair_count": pair_count,
         "first_episode": first_pair * 2, "episode_count": pair_count * 2,
         "stem": f"{label}-p{first_pair:06d}-n{pair_count:03d}"}
        for first_pair, pair_count in chunks
        for label in sorted(labels)
    ]


def _exec_argument_string(parts: list[str]) -> str:
    def quote(value: str) -> str:
        if not value or any(character.isspace() for character in value) or '"' in value:
            return '"' + value.replace('"', '\\"') + '"'
        return value
    return " ".join(quote(part) for part in parts)


def anchor_command(args: argparse.Namespace, model: dict[str, Any], first_pair: int,
                   pair_count: int, outcome: Path) -> list[str]:
    mode = model.get("mode", "population")
    if mode not in MODEL_AUTHORITY_MODES:
        fail(f"invalid model authority mode: {mode}")
    # --store-root (original) issues checkpoint_shadow_stdio_v1's
    # OriginalPromoted2StoreGeneration authority when --generation is also
    # given (the promoted(2) lineage pin): source_run_sha256 must equal its
    # own hard-coded constant, so this is only ever usable for the one
    # canonical promoted(2) store. --population-store-root (population)
    # issues PopulationStoreGeneration: any environment-randomization-v2
    # store, generation required either way.
    # population -> --population-store-root ROOT --generation N
    # original   -> --store-root ROOT --generation N (Java's own flag name;
    #               Java itself translates this to --original-store-root when
    #               it builds the Rust bridge command)
    # fixedstate -> --outcome-root ROOT, no --generation at all: Java rejects
    #               --generation alongside --outcome-root (Args.parse), and
    #               the Rust CLI rejects --generation alongside
    #               --xmage-cp7-outcome-root the same way (parse_args_v1,
    #               bin/checkpoint_shadow_stdio_v1.rs). ROOT is the staging
    #               directory, not a population-Store layout.
    # Per-model generation (model["generation"], set by load_store_identity):
    # every model in a group shares one panel-level --generation in the
    # common case, but a group built from per-spec GENERATION values (see
    # parse_model_spec / main()'s mixing check) can legitimately differ
    # model to model, so this must never read the shared args.generation
    # directly.
    if mode == "population":
        root_args = ["--population-store-root", model["root"], "--generation", str(model["generation"])]
    elif mode == "original":
        root_args = ["--store-root", model["root"], "--generation", str(model["generation"])]
    else:
        root_args = ["--outcome-root", model["root"]]
    execution_args = _exec_argument_string([
        "--repo-root", str(args.mage_repo), "--scorer-exe", str(args.scorer_exe),
        *root_args,
        "--base-seed", str(args.base_seed), "--first-episode", str(first_pair * 2),
        "--pairs", str(pair_count), "--opponent", "cp7", "--cp7-skill", "7",
        "--outcome-export", str(outcome),
    ])
    return [str(args.maven), "-o", "-q", "-pl", "Mage.Server.Plugins/Mage.Player.AIRL",
            "-DskipTests", "exec:java", "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
            "-Dexec.args=" + execution_args]


def environment(database_root: Path, model: dict[str, Any], *,
                tolerate_engine_faults: bool) -> dict[str, str]:
    value = os.environ.copy()
    value.update({"MAGE_DB_DIR": str(database_root), "MAGE_DB_AUTO_SERVER": "false",
                  "AI_DETERMINISTIC_TIEBREAKS": "true", "AI_DETERMINISTIC_SEARCH": "true",
                  "AI_DETERMINISTIC_MAX_NODES": "5000", "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
                  "CUDA_VISIBLE_DEVICES": "1"})
    # The xmage.rally.populationStore.* / xmage.rally.cp7Outcome.* system
    # properties are only read by the Java bridge client when it selects the
    # matching mode (XMageRallyBridgeProcessClient.fromSystemProperties,
    # gated on selectsPopulationStore / selectsXMageCp7Outcome respectively);
    # an --original-store-root model reads neither, so MAVEN_OPTS is left
    # unset rather than asserting properties the loaded authority mode does
    # not use.
    mode = model.get("mode", "population")
    if mode == "population":
        value["MAVEN_OPTS"] = maven_opts(model["checkpoint"])
    elif mode == "fixedstate":
        value["MAVEN_OPTS"] = xmage_cp7_outcome_maven_opts(model["checkpoint"])
    if tolerate_engine_faults:
        value["AI_ANCHOR_TOLERATE_ENGINE_FAULTS"] = "1"
    return value


def expected_header(checkpoint: dict[str, Any], schema_version: int = 2) -> dict[str, Any]:
    if schema_version not in (1, 2):
        fail(f"invalid outcome schema version: {schema_version}")
    return {
        "record_type": "header", "schema_version": schema_version, "record_ordinal": 0,
        "export_contract": OUTCOME_CONTRACT if schema_version == 2 else OUTCOME_CONTRACT_V1,
        "selection_source": "candidate_checkpoint_policy",
        "tensorizer_identity": "mtg-kernel-python-encoded-decision-tensor-contract-v2",
        "tensorizer_features_source_sha256":
            "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb",
        "model_input_commitment":
            "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1",
        "checkpoint": checkpoint,
    }


def _strict_json(raw: bytes, path: Path, line_number: int) -> dict[str, Any]:
    def object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                fail(f"{path}:{line_number}: duplicate JSON field {key}")
            value[key] = item
        return value
    try:
        row = json.loads(raw, object_pairs_hook=object_pairs,
                         parse_constant=lambda value: fail(
                             f"{path}:{line_number}: nonfinite JSON value {value}"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"{path}:{line_number}: invalid JSON: {error}")
    if not isinstance(row, dict):
        fail(f"{path}:{line_number}: row is not an object")
    return row


def _plain_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _validate_decision(path: Path, row: dict[str, Any], ordinal: int,
                       schema_version: int) -> None:
    expected_keys = DECISION_KEYS if schema_version == 2 else DECISION_KEYS_V1
    legal_count, selected = row.get("legal_action_count"), row.get("selected_index")
    semantics, logits = row.get("action_semantics"), row.get("old_policy_logits_f32_bits")
    if (set(row) != expected_keys or row.get("record_type") != "decision"
            or row.get("selection_source") != "candidate_checkpoint_policy"
            or not _plain_int(row.get("outcome_decision_ordinal"))
            or not _plain_int(legal_count) or legal_count < 1
            or not _plain_int(selected) or not 0 <= selected < legal_count
            or not isinstance(semantics, list) or len(semantics) != legal_count
            or row.get("selected_semantic") != semantics[selected]
            or not isinstance(logits, list) or len(logits) != legal_count
            or any(not _plain_int(value) for value in logits)
            or not isinstance(row.get("tensor"), dict)
            or HEX_64.fullmatch(row.get("model_input_sha256", "")) is None
            or HEX_32.fullmatch(row.get("candidate_order_commitment_128_hex", "")) is None
            or not _plain_int(row.get("old_value_f32_bits"))
            or row.get("acting_player") != row.get("candidate_seat")
            or not _plain_int(row.get("physical_decision_id"))
            or row["physical_decision_id"] < 0
            or not _plain_int(row.get("actor_physical_decision_ordinal"))
            or row["actor_physical_decision_ordinal"] < 0
            or not _plain_int(row.get("substep_index"))
            or not _plain_int(row.get("substep_count")) or row["substep_count"] < 1
            or not 0 <= row["substep_index"] < row["substep_count"]):
        fail(f"{path}: malformed outcome-v{schema_version} decision at record {ordinal}")


def _validate_terminal(path: Path, row: dict[str, Any], ordinal: int,
                       schema_version: int) -> str:
    expected_keys = TERMINAL_KEYS if schema_version == 2 else TERMINAL_KEYS_V1
    terminal, reward = row.get("terminal"), row.get("candidate_terminal_reward")
    if not isinstance(terminal, dict):
        fail(f"{path}: terminal payload missing at record {ordinal}")
    outcome = terminal.get("terminal_outcome")
    expected = {"p0_win": ("p0", [1, -1]), "p1_win": ("p1", [-1, 1]),
                "draw": (None, [0, 0])}.get(outcome)
    seat_index = 0 if row.get("candidate_seat") == "p0" else 1
    if (set(row) != expected_keys or row.get("record_type") != "terminal"
            or terminal.get("schema_version") != 5
            or terminal.get("episode_id") != row.get("episode_id")
            or terminal.get("terminal_classification") != "natural"
            or terminal.get("terminal_code") != "natural_game_over"
            or terminal.get("terminal_reason") != "game_over" or expected is None
            or terminal.get("winner") != expected[0]
            or terminal.get("terminal_reward") != expected[1]
            or reward not in (-1, 0, 1) or expected[1][seat_index] != reward
            or not _plain_int(row.get("outcome_decision_count"))
            or row["outcome_decision_count"] < 0):
        fail(f"{path}: terminal is not an exact outcome-v{schema_version} outcome at record {ordinal}")
    return "win" if reward == 1 else "draw" if reward == 0 else "loss"


def validate_outcome_shard(path: Path, model: dict[str, Any], *, base_seed: int,
                           first_pair: int, pair_count: int,
                           voided_pairs: frozenset[int] = frozenset()) -> dict[str, Any]:
    if not path.is_file() or not 0 <= base_seed <= 0x7FFF_FFFF_FFFF_FFFF:
        fail(f"invalid outcome path or base seed: {path}")
    task_pairs = range(first_pair, first_pair + pair_count)
    if not set(voided_pairs) <= set(task_pairs):
        fail(f"{path}: voided pair outside the task range")
    expected_checkpoint = model["checkpoint"]
    # Schema v1 (no per-row checkpoint block -- see the OUTCOME_CONTRACT_V1
    # comment above) is accepted ONLY for models bound via original mode.
    # population and fixedstate models always require the full v2 shape;
    # a v1 row from either is rejected outright, never silently accepted
    # under a looser check.
    expected_schema_version = 1 if model.get("mode", "population") == "original" else 2
    exact_header = expected_header(expected_checkpoint, schema_version=expected_schema_version)
    # Only non-voided pairs must appear, in full, in order. Rows that belong to
    # a voided pair are still fully schema-validated below (evidence, not
    # noise) but are never required to close with a terminal -- the engine
    # fault can leave a leg's decision stream open with no terminal at all,
    # or leave the pair's other, unaffected leg with a complete valid
    # terminal that must still be excluded -- and they never enter the
    # returned outcomes (both legs of a voided pair contribute nothing).
    required_sequence = [pair * 2 + seat for pair in task_pairs if pair not in voided_pairs
                         for seat in (0, 1)]
    required_index = 0
    decision_ordinal = 0
    active_episode: int | None = None
    active_episode_voided = False
    active_first_decision: int | None = None
    active_decision_count = 0
    terminal_keys: set[tuple[int, int, str]] = set()
    environment_seeds: dict[int, str] = {}
    outcomes: list[dict[str, Any]] = []
    record_count = 0
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            digest.update(raw)
            if not raw.endswith(b"\n") or b"\r" in raw or raw == b"\n":
                fail(f"{path}:{line_number}: noncanonical JSONL row")
            row = _strict_json(raw, path, line_number)
            if raw != (json.dumps(row, ensure_ascii=False, allow_nan=False,
                                  separators=(",", ":")) + "\n").encode("utf-8"):
                fail(f"{path}:{line_number}: JSONL row is not canonical compact JSON")
            ordinal = record_count
            record_count += 1
            if ordinal == 0:
                if row != exact_header:
                    fail(f"{path}: first row is not the exact population "
                         f"outcome-v{expected_schema_version} header")
                continue
            if row.get("record_type") == "header":
                fail(f"{path}: duplicate header at record {ordinal}")
            if (row.get("schema_version") != expected_schema_version
                    or row.get("record_ordinal") != ordinal
                    or (expected_schema_version == 2
                        and row.get("checkpoint") != expected_checkpoint)):
                fail(f"{path}: schema, ordinal, or checkpoint mismatch at record {ordinal}")
            pair, episode, seat = row.get("pair_index"), row.get("episode_id"), row.get("candidate_seat")
            if (not _plain_int(pair) or pair not in task_pairs
                    or seat not in {"p0", "p1"} or not _plain_int(episode)
                    or episode != pair * 2 + int(seat[1])
                    or row.get("base_seed_u64_hex") != f"{base_seed:016x}"
                    or HEX_16.fullmatch(row.get("pair_environment_seed_u64_hex", "")) is None):
                fail(f"{path}: pair, episode, seat, or seed mismatch at record {ordinal}")
            pair_voided = pair in voided_pairs
            if not pair_voided:
                if (required_index >= len(required_sequence)
                        or episode != required_sequence[required_index]):
                    fail(f"{path}: unexpected episode order at record {ordinal}")
            environment_seed = row["pair_environment_seed_u64_hex"]
            if pair in environment_seeds and environment_seeds[pair] != environment_seed:
                fail(f"{path}: pair environment seed changed for pair {pair}")
            environment_seeds[pair] = environment_seed
            if row.get("record_type") == "decision":
                _validate_decision(path, row, ordinal, expected_schema_version)
                if active_episode is None:
                    active_episode, active_episode_voided = episode, pair_voided
                    active_first_decision = decision_ordinal
                    active_decision_count = 0
                elif active_episode != episode:
                    fail(f"{path}: decisions are interleaved between episodes")
                if row["outcome_decision_ordinal"] != decision_ordinal:
                    fail(f"{path}: noncontiguous outcome decision ordinal")
                decision_ordinal += 1
                active_decision_count += 1
            elif row.get("record_type") == "terminal":
                result = _validate_terminal(path, row, ordinal, expected_schema_version)
                if active_episode is None:
                    active_episode, active_episode_voided = episode, pair_voided
                    active_first_decision = None
                    active_decision_count = 0
                if (active_episode != episode
                        or row.get("first_outcome_decision_ordinal") != active_first_decision
                        or row.get("outcome_decision_count") != active_decision_count):
                    fail(f"{path}: terminal does not exactly close the active episode")
                key = (pair, episode, seat)
                if key in terminal_keys:
                    fail(f"{path}: duplicate terminal {key}")
                terminal_keys.add(key)
                if not pair_voided:
                    outcomes.append({"pair_index": pair, "seat": seat, "result": result,
                                     "environment_seed": environment_seed})
                    required_index += 1
                active_episode = active_first_decision = None
                active_decision_count = 0
                active_episode_voided = False
            else:
                fail(f"{path}: unknown record type at record {ordinal}")
    if record_count == 0:
        fail(f"{path}: empty outcome file")
    if active_episode is not None and not active_episode_voided:
        fail(f"{path}: file ends mid-episode for a non-voided pair")
    if required_index != len(required_sequence):
        fail(f"{path}: terminal pair, episode, or seat coverage mismatch")
    required_seed_pairs = set(task_pairs) - set(voided_pairs)
    if not required_seed_pairs <= set(environment_seeds):
        fail(f"{path}: pair environment seed coverage mismatch")
    return {"sha256": digest.hexdigest(), "byte_count": path.stat().st_size,
            "first_pair": first_pair, "pair_count": pair_count,
            "record_count": record_count, "decision_count": decision_ordinal,
            "outcomes": outcomes, "environment_seeds": environment_seeds,
            "voided_pairs": sorted(voided_pairs),
            "schema_version": expected_schema_version}


class DatabaseLeasePool:
    def __init__(self, roots: list[Path]):
        if not roots:
            fail("database lease pool is empty")
        self._slots: queue.Queue[tuple[int, Path]] = queue.Queue()
        self._active: set[int] = set()
        self._lock = threading.Lock()
        for worker, root in enumerate(roots):
            self._slots.put((worker, root))

    def acquire(self) -> tuple[int, Path]:
        worker, root = self._slots.get()
        with self._lock:
            if worker in self._active:
                fail(f"database worker {worker} received an overlapping lease")
            self._active.add(worker)
        return worker, root

    def release(self, worker: int, root: Path) -> None:
        with self._lock:
            if worker not in self._active:
                fail(f"database worker {worker} released without an active lease")
            self._active.remove(worker)
        self._slots.put((worker, root))


def find_structured_lines(log_text: str, marker: str) -> list[str]:
    return [line for line in log_text.splitlines() if line.startswith(marker)]


def parse_structured_line(line: str, marker: str) -> dict[str, str]:
    if not line.startswith(marker):
        fail(f"structured line does not start with {marker!r}: {line!r}")
    remainder = line[len(marker):]
    if remainder and not remainder.startswith(" "):
        fail(f"malformed structured line: {line!r}")
    fields: dict[str, str] = {}
    for token in remainder.split():
        if "=" not in token:
            fail(f"malformed structured token: {token!r}")
        key, value = token.split("=", 1)
        if not key or not value or key in fields:
            fail(f"duplicate, empty key, or empty value in structured token: {token!r}")
        fields[key] = value
    return fields


def _int_field(fields: dict[str, str], key: str, line: str) -> int:
    try:
        return int(fields[key])
    except (KeyError, ValueError):
        fail(f"structured line has an invalid integer field {key!r}: {line!r}")
    raise AssertionError("unreachable")  # fail() always raises


PAIR_VOID_FIELDS = {
    "base_seed", "pair_index", "environment_seed", "failing_episode",
    "candidate_seat", "fault_class", "engine_error_message",
    "pairs_completed_before_void",
}
SPIKE_VOID_STOP_FIELDS = {
    "base_seed", "opponent", "cp7_skill", "first_episode", "pairs_requested",
    "pairs_completed", "games_completed", "voided_pair_index", "voided_episode",
    "elapsed_ms",
}


def parse_void_line(line: str, *, base_seed: int, first_pair: int,
                    pair_count: int) -> dict[str, Any]:
    fields = parse_structured_line(line, PAIR_VOID_MARKER)
    if set(fields) != PAIR_VOID_FIELDS:
        fail(f"void line has unexpected fields: {line!r}")
    # Conservative by construction: XMageRallyAnchorSpike only ever raises an
    # EngineCriticalFaultException (and therefore only ever emits this line)
    # for an exact-class IllegalStateException with the exact GameImpl
    # engine-fault message. A different fault_class here means either the
    # Java side or this line was tampered with or is out of contract; treat
    # it as ambiguous and stay fatal rather than silently voiding.
    if fields["fault_class"] != "IllegalStateException":
        fail(f"void line fault class is not the conservative engine signature: {line!r}")
    if _int_field(fields, "base_seed", line) != base_seed:
        fail(f"void line base seed does not match the task: {line!r}")
    pair_index = _int_field(fields, "pair_index", line)
    if not first_pair <= pair_index < first_pair + pair_count:
        fail(f"void line pair index is outside the requested range: {line!r}")
    failing_episode = _int_field(fields, "failing_episode", line)
    if failing_episode not in (pair_index * 2, pair_index * 2 + 1):
        fail(f"void line failing episode does not belong to its pair: {line!r}")
    if fields["candidate_seat"] not in ("p0", "p1"):
        fail(f"void line candidate seat is invalid: {line!r}")
    if HEX_16.fullmatch(fields["environment_seed"]) is None:
        fail(f"void line environment seed is malformed: {line!r}")
    pairs_completed_before_void = _int_field(fields, "pairs_completed_before_void", line)
    if not 0 <= pairs_completed_before_void <= pair_index - first_pair:
        fail(f"void line pairs-completed count is inconsistent with its pair index: {line!r}")
    return {
        "pair_index": pair_index,
        "environment_seed": fields["environment_seed"],
        "failing_episode": failing_episode,
        "candidate_seat": fields["candidate_seat"],
        "fault_class": fields["fault_class"],
        "engine_error_message": fields["engine_error_message"],
        "pairs_completed_before_void": pairs_completed_before_void,
    }


def parse_void_stop_line(line: str, *, base_seed: int, first_pair: int,
                         pair_count: int, first_episode: int) -> dict[str, Any]:
    fields = parse_structured_line(line, SPIKE_VOID_STOP_MARKER)
    if set(fields) != SPIKE_VOID_STOP_FIELDS:
        fail(f"void-stop line has unexpected fields: {line!r}")
    if _int_field(fields, "base_seed", line) != base_seed:
        fail(f"void-stop line base seed does not match the task: {line!r}")
    if _int_field(fields, "first_episode", line) != first_episode:
        fail(f"void-stop line first episode does not match the task: {line!r}")
    if _int_field(fields, "pairs_requested", line) != pair_count:
        fail(f"void-stop line pairs-requested does not match the task: {line!r}")
    pairs_completed = _int_field(fields, "pairs_completed", line)
    if not 0 <= pairs_completed < pair_count:
        fail(f"void-stop line pairs-completed is out of range: {line!r}")
    if _int_field(fields, "games_completed", line) != pairs_completed * 2:
        fail(f"void-stop line games-completed is inconsistent with pairs-completed: {line!r}")
    voided_pair_index = _int_field(fields, "voided_pair_index", line)
    if voided_pair_index != first_pair + pairs_completed:
        fail(f"void-stop line voided pair is not immediately after"
             f" the completed prefix: {line!r}")
    return {
        "pairs_completed": pairs_completed,
        "voided_pair_index": voided_pair_index,
        "voided_episode": _int_field(fields, "voided_episode", line),
    }


def run_task(args: argparse.Namespace, leases: DatabaseLeasePool, label: str,
             model: dict[str, Any], first_pair: int, pair_count: int) -> dict[str, Any]:
    worker, database = leases.acquire()
    try:
        task_root = args.evidence_root / "tasks"
        stem = f"{label}-p{first_pair:06d}-n{pair_count:03d}"
        segments: list[dict[str, Any]] = []
        voids: list[dict[str, Any]] = []
        remaining_first, remaining_count = first_pair, pair_count
        attempt = 0
        while remaining_count > 0:
            segment_stem = stem if attempt == 0 else f"{stem}-void{attempt:02d}"
            log = task_root / (segment_stem + ".log")
            outcome = task_root / (segment_stem + ".outcome.jsonl")
            started = time.perf_counter()
            with log.open("x", encoding="utf-8", newline="\n") as handle:
                completed = subprocess.run(
                    anchor_command(args, model, remaining_first, remaining_count, outcome),
                    cwd=args.mage_repo,
                    env=environment(database, model,
                                    tolerate_engine_faults=args.tolerate_engine_faults),
                    stdout=handle, stderr=subprocess.STDOUT,
                    timeout=args.task_timeout_seconds,
                )
            elapsed = time.perf_counter() - started
            if not outcome.is_file():
                fail(f"panel task failed: {label} pairs {remaining_first}+{remaining_count}"
                     f" (no outcome file, returncode={completed.returncode})")
            log_text = log.read_text(encoding="utf-8")
            pass_lines = find_structured_lines(log_text, SPIKE_PASS_MARKER)
            void_stop_lines = find_structured_lines(log_text, SPIKE_VOID_STOP_MARKER)
            void_lines = find_structured_lines(log_text, PAIR_VOID_MARKER)
            base_segment = {
                "label": label, "worker": worker, "elapsed_seconds": elapsed,
                "attempt": attempt, "returncode": completed.returncode,
                "log": str(log.resolve()), "log_sha256": sha256(log),
                "outcome": str(outcome.resolve()), "outcome_sha256": sha256(outcome),
            }
            if (completed.returncode == 0 and len(pass_lines) == 1
                    and not void_stop_lines and not void_lines):
                segments.append({**base_segment, "first_pair": remaining_first,
                                 "pair_count": remaining_count, "voided_pairs": []})
                remaining_count = 0
                break
            if not args.tolerate_engine_faults:
                fail(f"panel task failed: {label} pairs {remaining_first}+{remaining_count}"
                     f" returncode={completed.returncode}")
            if (completed.returncode == 0 and len(void_stop_lines) == 1
                    and len(void_lines) == 1 and not pass_lines):
                stop_info = parse_void_stop_line(
                    void_stop_lines[0], base_seed=args.base_seed,
                    first_pair=remaining_first, pair_count=remaining_count,
                    first_episode=remaining_first * 2)
                void_info = parse_void_line(
                    void_lines[0], base_seed=args.base_seed,
                    first_pair=remaining_first, pair_count=remaining_count)
                if (void_info["pair_index"] != stop_info["voided_pair_index"]
                        or void_info["pairs_completed_before_void"] != stop_info["pairs_completed"]
                        or void_info["failing_episode"] != stop_info["voided_episode"]):
                    fail(f"void marker and void-stop summary disagree: {label}"
                         f" pairs {remaining_first}+{remaining_count}")
                voided_pair = void_info["pair_index"]
                actual_count = stop_info["pairs_completed"] + 1
                segments.append({**base_segment, "first_pair": remaining_first,
                                 "pair_count": actual_count,
                                 "voided_pairs": [voided_pair]})
                void_record = dict(void_info)
                void_record["label"] = label
                voids.append(void_record)
                remaining_first = remaining_first + actual_count
                remaining_count = remaining_count - actual_count
                attempt += 1
                continue
            fail(f"panel task produced an ambiguous outcome: {label}"
                 f" pairs {remaining_first}+{remaining_count}"
                 f" returncode={completed.returncode} pass_lines={len(pass_lines)}"
                 f" void_stop_lines={len(void_stop_lines)} void_lines={len(void_lines)}")
        return {"label": label, "first_pair": first_pair, "pair_count": pair_count,
                "elapsed_seconds": sum(segment["elapsed_seconds"] for segment in segments),
                "segments": segments, "voids": voids}
    finally:
        leases.release(worker, database)


def aggregate_terminal_wdl(results: list[dict[str, Any]], labels: list[str]) -> dict[str, dict[str, Any]]:
    summary: dict[str, dict[str, Any]] = {}
    for label in labels:
        rows = [row for row in results if row["label"] == label]
        counts = {"win": 0, "draw": 0, "loss": 0}
        seats = {seat: {"win": 0, "draw": 0, "loss": 0} for seat in ("p0", "p1")}
        for row in rows:
            for seat, result in row["by_seat"].items():
                if seat not in seats or result not in counts:
                    fail("terminal aggregation input is invalid")
                counts[result] += 1
                seats[seat][result] += 1
        summary[label] = {"overall_wdl": counts, "by_seat_wdl": seats}
    return summary


def build_launch_plan(args: argparse.Namespace, identities: dict[str, dict[str, Any]],
                      tasks: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema": "mtg-kernel-population-store-cp7-panel-plan/v2",
        "status": "planned",
        "inputs": {
            "runner": str(Path(__file__).resolve()),
            "runner_sha256": sha256(Path(__file__).resolve()),
            "scorer_exe": str(args.scorer_exe.resolve()),
            "scorer_sha256": sha256(args.scorer_exe),
            "mage_repo": str(args.mage_repo.resolve()),
            "mage_commit": _git_commit(args.mage_repo),
            "source_database": str(args.source_database.resolve()),
            "source_database_sha256": sha256(args.source_database),
            "models": identities,
        },
        "panel": {
            "mode": args.mode, "opponent": "xmage-cp7", "cp7_skill": 7,
            "base_seed": args.base_seed, "pair_start": args.pair_start,
            "pair_count": args.pairs, "episode_count": args.pairs * 2,
            # args.generation is the single shared panel-level value, or null
            # when every model instead carries its own --model spec
            # GENERATION (see parse_model_spec / main()'s mixing check). The
            # per-model value actually used for each model, either way, is
            # always in inputs.models[label].generation above.
            "generation": args.generation,
            "model_generations": {label: identity["generation"]
                                  for label, identity in identities.items()},
            "workers": args.workers,
            "task_pairs": args.task_pairs, "task_count": len(tasks),
            "task_timeout_seconds": args.task_timeout_seconds,
            "tolerate_engine_faults": args.tolerate_engine_faults,
            "void_cap_fraction": VOID_CAP_FRACTION_NUMERATOR / VOID_CAP_FRACTION_DENOMINATOR,
            "tasks": tasks,
        },
        "toolchain": {
            "python": sys.version, "rustc": _version(["rustc", "-V"]),
            "maven": _version([str(args.maven), "-version"]),
        },
        "analysis_policy": {
            "parse_outcomes_after_all_shards_complete": True,
            "terminal_win_draw_loss_only": True,
            "promotion_claim": False,
        },
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    if args.evidence_root.exists():
        fail(f"evidence root already exists: {args.evidence_root}")
    if args.pairs != (1 if args.mode == "smoke" else 128):
        fail("smoke requires one pair and formal requires 128 pairs")
    if sha256(args.source_database) != CARD_DB_HASH:
        fail("card database hash mismatch")
    models = {label: (mode, generation, root) for label, mode, generation, root in args.models}
    identities = {label: load_store_identity(root, generation, mode=mode)
                  for label, (mode, generation, root) in models.items()}
    chunks = chunk_ranges(args.pair_start, args.pairs, args.task_pairs)
    task_plan = planned_tasks(list(identities), chunks)
    args.evidence_root.mkdir(parents=True)
    plan_path = args.evidence_root / "panel-plan.json"
    launch_plan = build_launch_plan(args, identities, task_plan)
    exclusive_write(plan_path, canonical_json_bytes(launch_plan, indent=2))
    (args.evidence_root / "tasks").mkdir()
    workers: list[Path] = []
    for worker in range(args.workers):
        root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True)
        target = root / "cards.h2.mv.db"
        shutil.copyfile(args.source_database, target)
        if sha256(target) != CARD_DB_HASH:
            fail("worker card database copy hash mismatch")
        workers.append(root)
    leases = DatabaseLeasePool(workers)
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = [executor.submit(run_task, args, leases, task["label"],
                                   identities[task["label"]], task["first_pair"],
                                   task["pair_count"])
                   for task in task_plan]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda row: (row["first_pair"], row["label"]))
    completed = [(row["label"], row["first_pair"], row["pair_count"]) for row in results]
    expected = [(task["label"], task["first_pair"], task["pair_count"])
                for task in task_plan]
    if completed != expected:
        fail("completed shard coverage differs from the create-new launch plan")

    pair_results: dict[tuple[str, int], dict[str, Any]] = {}
    all_voids: list[dict[str, Any]] = []
    model_void_counts = {label: 0 for label in identities}
    model_schema_versions: dict[str, set[int]] = {label: set() for label in identities}
    for result in results:
        for segment in result["segments"]:
            voided_pairs = frozenset(segment["voided_pairs"])
            validation = validate_outcome_shard(
                Path(segment["outcome"]), identities[result["label"]],
                base_seed=args.base_seed, first_pair=segment["first_pair"],
                pair_count=segment["pair_count"], voided_pairs=voided_pairs,
            )
            model_schema_versions[result["label"]].add(validation["schema_version"])
            segment["validation"] = {
                key: value for key, value in validation.items()
                if key not in {"outcomes", "environment_seeds"}
            }
            for outcome in validation["outcomes"]:
                key = (result["label"], outcome["pair_index"])
                row = pair_results.setdefault(key, {
                    "label": result["label"], "pair_index": outcome["pair_index"],
                    "environment_seed": outcome["environment_seed"], "by_seat": {},
                })
                if (row["environment_seed"] != outcome["environment_seed"]
                        or outcome["seat"] in row["by_seat"]):
                    fail("duplicate terminal or pair environment seed mismatch across shards")
                row["by_seat"][outcome["seat"]] = outcome["result"]
        for void_record in result["voids"]:
            all_voids.append(void_record)
            model_void_counts[result["label"]] += 1

    for label in identities:
        voided = model_void_counts[label]
        if voided * VOID_CAP_FRACTION_DENOMINATOR > args.pairs * VOID_CAP_FRACTION_NUMERATOR:
            fail(f"void rate for model {label} exceeds the "
                 f"{VOID_CAP_FRACTION_NUMERATOR}% cap: {voided}/{args.pairs} pairs voided")

    # Every segment's outcome rows for a given model are validated against the
    # same mode-derived expected_schema_version (original -> v1, population and
    # fixedstate -> v2), so a model can never carry a mix of schema versions
    # across its segments; this is a defensive cross-check of that invariant,
    # not a new source of truth.
    outcome_schema_versions: dict[str, int] = {}
    for label, versions in model_schema_versions.items():
        if len(versions) != 1:
            fail(f"model {label} outcome rows carried inconsistent schema versions: {sorted(versions)}")
        outcome_schema_versions[label] = next(iter(versions))

    pairs = list(range(args.pair_start, args.pair_start + args.pairs))
    voided_pair_keys = {(record["label"], record["pair_index"]) for record in all_voids}
    expected_pair_keys = {(label, pair) for label in identities for pair in pairs} - voided_pair_keys
    if set(pair_results) != expected_pair_keys or any(
        set(row["by_seat"]) != {"p0", "p1"} for row in pair_results.values()
    ):
        fail("parsed model, pair, or seat coverage is incomplete")
    terminal_results = sorted(pair_results.values(),
                              key=lambda row: (row["pair_index"], row["label"]))
    summary = aggregate_terminal_wdl(terminal_results, sorted(identities))
    for pair in pairs:
        seeds = {row["environment_seed"] for row in terminal_results
                 if row["pair_index"] == pair}
        # A pair voided for every model leaves no seed to cross-check; that is
        # fine (nothing to compare). More than one distinct seed among the
        # models that DID complete the pair is a real integrity failure.
        if len(seeds) > 1:
            fail(f"models did not share pair environment seed for pair {pair}")

    void_summary = {
        label: {
            "pairs_requested": args.pairs,
            "voided_pairs": model_void_counts[label],
            "voided_pair_indices": sorted(
                record["pair_index"] for record in all_voids if record["label"] == label),
        }
        for label in identities
    }
    manifest = {
        "schema": "mtg-kernel-population-store-cp7-panel/v3", "mode": args.mode,
        "base_seed": args.base_seed, "pair_start": args.pair_start, "pairs": args.pairs,
        "workers": args.workers, "task_pairs": args.task_pairs,
        "tolerate_engine_faults": args.tolerate_engine_faults,
        "plan": {"path": str(plan_path.resolve()), "sha256": sha256(plan_path)},
        "tasks": results, "terminal_wdl": summary,
        "outcome_schema_versions": outcome_schema_versions,
        "voids": {
            "schema": "mtg-kernel-cp7-panel-pair-void/v1",
            "cap_fraction": VOID_CAP_FRACTION_NUMERATOR / VOID_CAP_FRACTION_DENOMINATOR,
            "per_model": void_summary,
            "records": all_voids,
            "bias_caveat": (
                "Voided pairs are environment-caused exclusions (a deterministic "
                "XMage engine critical-error fault, not a model or candidate-policy "
                "failure). They are dropped from terminal_wdl entirely: neither leg "
                "of a voided pair counts as a win, loss, or draw for any model. If "
                "per_model voided_pairs counts are asymmetric across models, that "
                "asymmetry is a bias caveat on this panel and any near-margin "
                "cross-model comparison should be treated with additional scrutiny."
            ),
        },
        "non_claims": ["terminal win/loss/draw is the only playing-strength outcome",
                       "this external CP7 anchor panel is not a promotion or professional-level claim",
                       "voided pairs are excluded from outcomes, not adjudicated as a result"],
    }
    output = args.evidence_root / "panel-summary.json"
    exclusive_write(output, canonical_json_bytes(manifest, indent=2))
    return manifest


def self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "store"; (root / "checkpoints").mkdir(parents=True)
        def write_json(path: Path, value: dict[str, Any]) -> None:
            with path.open("w", encoding="utf-8", newline="\n") as handle:
                handle.write(json.dumps(value, sort_keys=True) + "\n")
        generation = 1024; state = b"state"; state_sha = hashlib.sha256(state).hexdigest()
        run = {"schema": "mtg_kernel_native_train_run/v2", "store_identity": "mtg-kernel-native-training-store-v2",
               "contracts": {"identity_bundle_sha256": "a" * 64,
                             "learner_sampler": {"identity": SAMPLER_IDENTITY, "contract_sha256": SAMPLER_CONTRACT}}}
        write_json(root / "run.json", run)
        run_sha = sha256(root / "run.json")
        checkpoint = {"schema": "mtg_kernel_native_train_checkpoint/v3", "generation_index": generation,
                      "identity_bundle_sha256": "a" * 64, "run_sha256": run_sha,
                      "payload": {"byte_count": 5, "sha256": state_sha,
                                  "sections": [{"sha256": "1" * 64}, {"sha256": "2" * 64}, {"sha256": "3" * 64}]},
                      "train_state": {"state_sha256": "4" * 64, "model_parameter_sha256": "5" * 64}}
        checkpoint_path = root / "checkpoints" / "update-00001024.checkpoint.json"
        write_json(checkpoint_path, checkpoint)
        checkpoint_sha = sha256(checkpoint_path)
        sidecar = {"schema": "mtg_kernel_native_train_checkpoint_sidecar/v2", "generation_index": generation,
                   "run_sha256": run_sha, "checkpoint_manifest_sha256": checkpoint_sha,
                   "checkpoint_payload_sha256": state_sha, "train_state_sha256": "4" * 64,
                   "model_parameter_sha256": "5" * 64}
        write_json(root / "checkpoints" / "update-00001024.sidecar.json", sidecar)
        (root / "checkpoints" / "update-00001024.state.f32le").write_bytes(state)
        write_json(root / "latest.json", {"schema": "mtg_kernel_native_train_latest/v2", "generation_index": generation + 4, "run_sha256": run_sha, "identity_bundle_sha256": "a" * 64})
        identity = load_store_identity(root, generation)
        assert identity["checkpoint"]["loaded_generation"] == generation
        assert identity["store_files"]["store_head_generation"] == generation + 4
        try:
            load_store_identity(root, generation + 4)
            raise AssertionError("missing generation was accepted")
        except ValueError:
            pass
        aggregate = aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        assert aggregate["a"]["overall_wdl"] == {"win": 1, "draw": 2, "loss": 1}
        void_line = ("XMAGE_RALLY_ANCHOR_PAIR_VOID base_seed=7 pair_index=66"
                     " environment_seed=3d403c464bfa8dca failing_episode=133"
                     " candidate_seat=p1 fault_class=IllegalStateException"
                     " engine_error_message=Error_in_unit_tests"
                     " pairs_completed_before_void=2")
        void_info = parse_void_line(void_line, base_seed=7, first_pair=64, pair_count=32)
        assert void_info["pair_index"] == 66
        assert void_info["pairs_completed_before_void"] == 2
        stop_line = ("XMAGE_RALLY_ANCHOR_SPIKE VOID_STOP base_seed=7 opponent=cp7"
                     " cp7_skill=7 first_episode=128 pairs_requested=32"
                     " pairs_completed=2 games_completed=4 voided_pair_index=66"
                     " voided_episode=133 elapsed_ms=1000")
        stop_info = parse_void_stop_line(stop_line, base_seed=7, first_pair=64,
                                         pair_count=32, first_episode=128)
        assert stop_info["voided_pair_index"] == 66
    print("PASS retained population Store generation and missing-generation rejection")
    return 0


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--self-test", action="store_true")
    value.add_argument("--evidence-root", type=Path)
    value.add_argument("--model", action="append", dest="model_specs", default=[])
    value.add_argument("--generation", type=int)
    value.add_argument("--mode", choices=("smoke", "formal"))
    value.add_argument("--base-seed", type=int)
    value.add_argument("--pair-start", type=int, default=0)
    value.add_argument("--pairs", type=int)
    value.add_argument("--workers", type=int, choices=(1, 8), default=8)
    value.add_argument("--task-pairs", type=int, default=32)
    value.add_argument("--task-timeout-seconds", type=int, default=1800)
    value.add_argument("--scorer-exe", type=Path)
    value.add_argument("--mage-repo", type=Path)
    value.add_argument("--source-database", type=Path)
    value.add_argument("--maven", type=Path)
    value.add_argument("--tolerate-engine-faults", action="store_true", default=False,
                       help="Void a pair (both legs) on XMage's own critical-error"
                            " signature instead of failing the whole campaign."
                            " Sealed off by default: unset, behavior is identical"
                            " to run_cp7_store_panel_v1.py.")
    return value


GENERATION_PREFIX = re.compile(r"^([0-9]+):")


def parse_model_spec(spec: str) -> tuple[str, str, int | None, Path]:
    """Parses one --model spec: label=[MODE:][GENERATION:]STORE_ROOT.

    MODE is one of MODEL_AUTHORITY_MODES ("population", "original", or
    "fixedstate"), defaulting to "population" when no recognized MODE: prefix
    is present -- this is the exact bareword label=STORE_ROOT shape every
    existing invocation already uses, so it keeps working unchanged. Matching
    is a literal startswith check against the known prefixes, never a blind
    split on the first colon, because a Windows store root routinely starts
    with its own drive-letter colon (e.g. "D:\\...") that must not be
    mistaken for a mode prefix.

    GENERATION, if present, is a decimal run immediately followed by a colon,
    checked after the mode prefix is stripped. This can never collide with a
    real store root either: Windows drive letters are always a single ASCII
    letter, never a digit, so a path can never start with digits-then-colon.
    Returns None for GENERATION when the spec does not carry one -- the
    caller (main()) decides whether that is allowed, since whether a
    per-spec generation is required, forbidden, or optional depends on
    whether --generation was also given and on what every OTHER --model spec
    in the same invocation did (see main()'s mixing check).
    """
    if spec.count("=") != 1:
        fail("model must be label=[MODE:][GENERATION:]STORE_ROOT")
    label, raw_root = spec.split("=", 1)
    if not label or not label.replace("-", "").replace("_", "").isalnum():
        fail("model label is invalid")
    mode = "population"
    remainder = raw_root
    for candidate in MODEL_AUTHORITY_MODES:
        prefix = candidate + ":"
        if raw_root.startswith(prefix):
            mode = candidate
            remainder = raw_root[len(prefix):]
            break
    generation: int | None = None
    generation_match = GENERATION_PREFIX.match(remainder)
    if generation_match is not None:
        generation = int(generation_match.group(1))
        remainder = remainder[generation_match.end():]
    if not remainder:
        fail("model store root is empty")
    return label, mode, generation, Path(remainder)


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.self_test:
        return self_test()
    # --generation is no longer unconditionally required here: it becomes
    # optional exactly when every --model spec carries its own GENERATION
    # (see the mixing check below). Every other panel argument is still
    # mandatory as before.
    required = (args.evidence_root, args.mode, args.base_seed,
                args.pairs, args.scorer_exe, args.mage_repo, args.source_database, args.maven)
    if (any(value is None for value in required)
            or not 0 <= args.base_seed <= 0x7FFF_FFFF_FFFF_FFFF
            or args.pair_start < 0 or not 1 <= args.task_pairs <= 128
            or args.task_timeout_seconds < 60):
        fail("missing or invalid panel arguments")
    parsed: list[tuple[str, str, int | None, Path]] = [
        parse_model_spec(spec) for spec in args.model_specs]
    if len(parsed) != 3 or len({label for label, _, _, _ in parsed}) != 3:
        fail("exactly three uniquely labelled models are required")
    if len({root.resolve() for _, _, _, root in parsed}) != 3:
        fail("three distinct population Store roots are required")
    # Exactly two valid forms, no mixing, fail-closed on any ambiguity:
    #   (a) shared form: --generation N given at the panel level, and every
    #       --model spec omits GENERATION (relies on the shared value).
    #   (b) per-model form: --generation omitted at the panel level, and
    #       every --model spec carries its own GENERATION.
    # A spec-level GENERATION together with a panel-level --generation is
    # rejected even if the two values would happen to agree: which one is
    # authoritative must never be a judgment call at read time. Likewise,
    # some specs carrying GENERATION while others do not is rejected outright
    # rather than silently falling back to the panel-level value for the
    # ones missing it.
    per_spec_generations = [generation for _, _, generation, _ in parsed]
    all_have_generation = all(value is not None for value in per_spec_generations)
    any_have_generation = any(value is not None for value in per_spec_generations)
    if any_have_generation and not all_have_generation:
        fail("either every --model spec carries its own GENERATION or none do")
    if all_have_generation and args.generation is not None:
        fail("--generation must be omitted when every --model spec carries its own GENERATION")
    if not all_have_generation and args.generation is None:
        fail("--generation is required when no --model spec carries its own GENERATION")
    args.models = [
        (label, mode, generation if generation is not None else args.generation, root)
        for label, mode, generation, root in parsed
    ]
    run(args)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"panel runner failed: {error}", file=sys.stderr)
        raise SystemExit(2)
