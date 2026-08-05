from __future__ import annotations

import copy
from pathlib import Path

from outcome_v2 import EXPECTED_HEADER, canonical_json_bytes


def decision(
    *,
    record_ordinal: int,
    decision_ordinal: int,
    pair: int,
    seat: str,
    base_seed: int,
) -> dict:
    episode = pair * 2 + int(seat[1])
    semantic = {"action_kind": "pass", "actor": seat}
    return {
        "record_type": "decision",
        "schema_version": 2,
        "record_ordinal": record_ordinal,
        "outcome_decision_ordinal": decision_ordinal,
        "pair_index": pair,
        "episode_id": episode,
        "candidate_seat": seat,
        "base_seed_u64_hex": f"{base_seed:016x}",
        "pair_environment_seed_u64_hex": f"{pair + 100:016x}",
        "deck_ids": ["Rally", "Rally"],
        "environment_revision": 0,
        "randomization_identity": "environment-randomization-v2",
        "selection_source": "candidate_checkpoint_policy",
        "acting_player": seat,
        "step": 0,
        "decision_kind": "surface",
        "physical_decision_id": 0,
        "actor_physical_decision_ordinal": 0,
        "substep_index": 0,
        "substep_count": 1,
        "legal_action_count": 1,
        "selected_index": 0,
        "selected_semantic": semantic,
        "candidate_order_commitment_128_hex": "1" * 32,
        "action_semantics": [semantic],
        "tensor": {},
        "model_input_sha256": f"{episode + 1:064x}",
        "old_policy_logits_f32_bits": [0],
        "old_value_f32_bits": 0,
        "checkpoint": copy.deepcopy(EXPECTED_HEADER["checkpoint"]),
    }


def terminal(
    *,
    record_ordinal: int,
    decision_ordinal: int,
    pair: int,
    seat: str,
    base_seed: int,
) -> dict:
    episode = pair * 2 + int(seat[1])
    return {
        "record_type": "terminal",
        "schema_version": 2,
        "record_ordinal": record_ordinal,
        "pair_index": pair,
        "episode_id": episode,
        "candidate_seat": seat,
        "base_seed_u64_hex": f"{base_seed:016x}",
        "pair_environment_seed_u64_hex": f"{pair + 100:016x}",
        "deck_ids": ["Rally", "Rally"],
        "randomization_identity": "environment-randomization-v2",
        "core_environment_hash_u64_hex": "0" * 16,
        "diagnostic_state_hash_u64_hex": "0" * 16,
        "first_outcome_decision_ordinal": decision_ordinal,
        "outcome_decision_count": 1,
        "terminal": {
            "schema_version": 5,
            "deck_ids": ["Rally", "Rally"],
            "deck_hashes": [1, 1],
            "episode_id": episode,
            "terminal_outcome": "p0_win",
            "terminal_classification": "natural",
            "terminal_code": "natural_game_over",
            "winner": "p0",
            "terminal_reward": [1, -1],
            "terminal_reason": "game_over",
            "policy_step_count": 1,
            "physical_decision_count": 1,
        },
        "candidate_terminal_reward": 1 if seat == "p0" else -1,
        "checkpoint": copy.deepcopy(EXPECTED_HEADER["checkpoint"]),
    }


def shard_rows(pair: int, base_seed: int) -> list[dict]:
    return [
        copy.deepcopy(EXPECTED_HEADER),
        decision(
            record_ordinal=1,
            decision_ordinal=0,
            pair=pair,
            seat="p0",
            base_seed=base_seed,
        ),
        terminal(
            record_ordinal=2,
            decision_ordinal=0,
            pair=pair,
            seat="p0",
            base_seed=base_seed,
        ),
        decision(
            record_ordinal=3,
            decision_ordinal=1,
            pair=pair,
            seat="p1",
            base_seed=base_seed,
        ),
        terminal(
            record_ordinal=4,
            decision_ordinal=1,
            pair=pair,
            seat="p1",
            base_seed=base_seed,
        ),
    ]


def write_rows(path: Path, rows: list[dict]) -> None:
    path.write_bytes(b"".join(canonical_json_bytes(row) for row in rows))
