from __future__ import annotations

import json
import os
import sys
import time

from fixtures import combat_decision_response, decision_response, stable_ref, terminal_response


COMBAT_SCENARIOS = {
    "combat_actor_drift",
    "combat_attacker_drift",
    "combat_count_drift",
    "combat_current_drift",
    "combat_id_drift",
    "combat_index_drift",
    "combat_selected_history_drift",
    "combat_selected_order_drift",
    "combat_stage_drift",
    "combat_suffix_drift",
    "combat_terminal_midgroup",
    "combat_then_surface",
    "combat_valid",
}


def combat_candidates(actor: str = "p0") -> list[dict]:
    return [
        stable_ref(201, 21, actor, "Battlefield"),
        stable_ref(202, 22, actor, "Battlefield"),
        stable_ref(203, 23, actor, "Battlefield"),
    ]


def emit(value: dict) -> None:
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def main() -> int:
    start_marker = os.environ.get("FAKE_START_MARKER")
    if start_marker:
        with open(start_marker, "w", encoding="utf-8") as fh:
            fh.write("started\n")
    scenario = os.environ.get("FAKE_SCENARIO", "valid")
    scenario_file = os.environ.get("FAKE_SCENARIO_FILE")
    if scenario_file:
        with open(scenario_file, "r", encoding="utf-8") as fh:
            scenario = fh.read().strip()
    request_log = os.environ.get("FAKE_REQUEST_LOG")
    if scenario == "timeout":
        time.sleep(60)
        return 0
    if scenario == "eof_nonzero":
        return 17
    count = 0
    episode_steps: dict[int, int] = {}
    episode_combat_selected: dict[int, list[int]] = {}
    for line in sys.stdin:
        if scenario_file:
            with open(scenario_file, "r", encoding="utf-8") as fh:
                scenario = fh.read().strip()
        req = json.loads(line)
        if request_log:
            with open(request_log, "a", encoding="utf-8", newline="\n") as fh:
                fh.write(json.dumps(req, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
        count += 1
        if scenario == "duplicate_keys":
            sys.stdout.write('{"response_type":"decision","response_type":"decision"}\n')
            sys.stdout.flush()
            continue
        if scenario == "nonfinite_json":
            sys.stdout.write('{"response_type":"decision","schema_version":NaN}\n')
            sys.stdout.flush()
            continue
        if scenario == "nonfinite_overflow":
            sys.stdout.write('{"response_type":"decision","schema_version":1e999}\n')
            sys.stdout.flush()
            continue
        if scenario == "noise":
            sys.stdout.write("not json\n")
            sys.stdout.flush()
            continue
        if scenario == "error_valid":
            emit({"response_type": "error", "schema_version": 5, "request_id": req["request_id"], "error": {"code": "bad_request", "message": "line one\nline two"}})
            continue
        if scenario == "error_legacy_v3":
            emit({"response_type": "error", "schema_version": 3, "request_id": req["request_id"], "error": {"code": "bad_request", "message": "bad"}})
            continue
        if scenario == "error_legacy_v4":
            emit({"response_type": "error", "schema_version": 4, "request_id": req["request_id"], "error": {"code": "bad_request", "message": "bad"}})
            continue
        if scenario == "error_bad_schema":
            emit({"response_type": "error", "schema_version": 1, "request_id": req["request_id"], "error": {"code": "bad_request", "message": "bad"}})
            continue
        if scenario == "error_bad_request_id":
            emit({"response_type": "error", "schema_version": 5, "request_id": "wrong", "error": {"code": "bad_request", "message": "bad"}})
            continue
        if scenario == "error_empty_code":
            emit({"response_type": "error", "schema_version": 5, "request_id": req["request_id"], "error": {"code": "", "message": "bad"}})
            continue
        if req["request_type"] == "reset":
            if req.get("deck_ids") != ["Burn", "Burn"]:
                emit({"response_type": "error", "schema_version": 5, "request_id": req["request_id"], "error": {"code": "unsupported_deck", "message": "fake environment only supports Burn/Burn"}})
                continue
            episode_steps[req["episode_id"]] = 0
            if scenario == "train_pair_slow":
                time.sleep(2.0)
            if scenario == "train_zero_learner":
                learner = "p0" if req["episode_id"] % 2 == 0 else "p1"
                actor = "p1" if learner == "p0" else "p0"
            else:
                actor = "p0"
            if scenario in COMBAT_SCENARIOS:
                episode_combat_selected[req["episode_id"]] = []
                stage = "blocker_inclusion" if scenario == "combat_attacker_drift" else "attacker_inclusion"
                emit(
                    combat_decision_response(
                        req["request_id"],
                        req["episode_id"],
                        0,
                        0,
                        actor=actor,
                        stage=stage,
                    )
                )
                continue
            resp = decision_response(req["request_id"], req["episode_id"], 0, actor=actor)
            if scenario == "extra_field":
                resp["extra"] = True
            elif scenario == "missing_field":
                del resp["reward"]
            elif scenario == "bool_int":
                resp["schema_version"] = True
            elif scenario == "u64_overflow":
                resp["episode_id"] = 18446744073709551616
            elif scenario == "episode_drift":
                resp["episode_id"] = req["episode_id"] + 1
            elif scenario == "step_drift":
                resp["step"] = 1
                resp["observation"]["step_index"] = 1
            elif scenario == "empty_actions":
                resp["legal_actions"] = []
            elif scenario == "noncontiguous_actions":
                resp["legal_actions"][1]["selected_index"] = 9
            elif scenario == "duplicate_actions":
                resp["legal_actions"][1]["stable_id"] = resp["legal_actions"][0]["stable_id"]
            elif scenario == "mismatched_action_actor":
                resp["legal_actions"][1]["semantic"]["actor"] = "p1"
            elif scenario == "mixed_action_actors":
                resp["legal_actions"].append(resp["legal_actions"][1].copy())
                resp["legal_actions"][3] = json.loads(json.dumps(resp["legal_actions"][3]))
                resp["legal_actions"][3]["selected_index"] = 3
                resp["legal_actions"][3]["stable_id"] = "legal-action-v5:d"
                resp["legal_actions"][3]["semantic"]["actor"] = "p1"
            elif scenario == "nonzero_reward":
                resp["reward"] = [1, 0]
            elif scenario == "substep_index_u32_overflow":
                resp["substep_index"] = 4_294_967_296
                resp["observation"]["substep_index"] = 4_294_967_296
            elif scenario == "substep_count_u32_overflow":
                resp["substep_count"] = 4_294_967_296
                resp["observation"]["substep_count"] = 4_294_967_296
            elif scenario == "wire_environment_hash":
                resp["environment_hash"] = 1
            elif scenario == "observation_environment_hash":
                resp["observation"]["environment_hash"] = 1
            elif scenario == "provenance_environment_hash_algorithm":
                resp["provenance"]["environment_hash_algorithm"] = "sha256"
            elif scenario == "decision_legacy_v4":
                resp["schema_version"] = 4
            elif scenario == "provenance_legacy_v4":
                resp["provenance"]["protocol_version"] = 4
                resp["provenance"]["schema_version"] = 4
            elif scenario == "observation_legacy_v4":
                resp["observation"]["schema_version"] = 4
            elif scenario == "action_legacy_v4":
                for action in resp["legal_actions"]:
                    action["schema_version"] = 4
                    action["stable_id"] = action["stable_id"].replace("legal-action-v5:", "legal-action-v4:")
            elif scenario == "deck_id_drift":
                resp["deck_ids"][1] = "Rally"
            elif scenario == "deck_hash_shape":
                resp["deck_hashes"] = [resp["deck_hashes"][0]]
            emit(resp)
        else:
            if scenario == "train_pair_assert_latest0" and count == 2:
                latest_path = os.environ.get("FAKE_EXPECT_LATEST_JSON")
                if not latest_path:
                    raise RuntimeError("FAKE_EXPECT_LATEST_JSON not set")
                with open(latest_path, "r", encoding="utf-8") as fh:
                    latest = json.load(fh)
                if latest.get("update") != 0:
                    raise RuntimeError(f"latest.json update was not 0 before first action: {latest!r}")
            expected_step = req["expected_step"] + 1
            episode_steps[req["episode_id"]] = expected_step
            if scenario in COMBAT_SCENARIOS:
                if scenario == "combat_terminal_midgroup":
                    emit(
                        terminal_response(
                            req["request_id"],
                            req["episode_id"],
                            expected_step,
                            physical_decisions=0,
                        )
                    )
                    continue
                selected = episode_combat_selected[req["episode_id"]]
                if req["selected_index"] == 1:
                    selected.append(expected_step - 1)
                if scenario == "combat_then_surface" and expected_step == 3:
                    emit(
                        decision_response(
                            req["request_id"],
                            req["episode_id"],
                            expected_step,
                            actor="p0",
                            physical_decision_id=1,
                            substep_index=0,
                            substep_count=1,
                        )
                    )
                    continue
                if expected_step >= (4 if scenario == "combat_then_surface" else 3):
                    emit(
                        terminal_response(
                            req["request_id"],
                            req["episode_id"],
                            expected_step,
                            physical_decisions=2 if scenario == "combat_then_surface" else 1,
                        )
                    )
                    continue
                actor = "p1" if scenario == "combat_actor_drift" and expected_step == 1 else "p0"
                stage = "blocker_inclusion" if scenario == "combat_attacker_drift" else "attacker_inclusion"
                attacker = None
                candidates = combat_candidates(actor)
                candidate_index = expected_step
                physical_decision_id = 0
                selected_for_response = tuple(selected)
                if expected_step == 1:
                    if scenario == "combat_stage_drift":
                        stage = "blocker_inclusion"
                    elif scenario == "combat_attacker_drift":
                        attacker = stable_ref(902, 32, "p1", "Battlefield")
                    elif scenario == "combat_current_drift":
                        candidates[1] = stable_ref(204, 24, actor, "Battlefield")
                    elif scenario == "combat_suffix_drift":
                        candidates[2] = stable_ref(204, 24, actor, "Battlefield")
                    elif scenario == "combat_selected_history_drift":
                        selected_for_response = ()
                    elif scenario == "combat_id_drift":
                        physical_decision_id = 1
                    elif scenario == "combat_index_drift":
                        candidate_index = 2
                    elif scenario == "combat_count_drift":
                        candidates.append(stable_ref(204, 24, actor, "Battlefield"))
                if scenario == "combat_selected_order_drift" and expected_step == 2:
                    selected_for_response = tuple(reversed(selected))
                emit(
                    combat_decision_response(
                        req["request_id"],
                        req["episode_id"],
                        expected_step,
                        candidate_index,
                        actor=actor,
                        physical_decision_id=physical_decision_id,
                        stage=stage,
                        candidates=candidates,
                        selected_indices=selected_for_response,
                        attacker=attacker,
                    )
                )
            elif scenario in ("train_pair", "train_pair_assert_latest0", "train_pair_slow"):
                if expected_step == 1:
                    emit(decision_response(req["request_id"], req["episode_id"], expected_step, actor="p1"))
                else:
                    outcomes = ["p0_win", "p1_win", "draw"]
                    emit(terminal_response(req["request_id"], req["episode_id"], expected_step, outcome=outcomes[req["episode_id"] % 3]))
            elif scenario == "train_zero_learner":
                outcomes = ["p0_win", "p1_win", "draw"]
                emit(terminal_response(req["request_id"], req["episode_id"], expected_step, outcome=outcomes[req["episode_id"] % 3]))
            elif scenario == "train_late_fault":
                if req["episode_id"] == 1:
                    resp = terminal_response(req["request_id"], req["episode_id"], expected_step)
                    resp["terminal_outcome"] = "halted"
                    resp["terminal_classification"] = "halted"
                    resp["terminal_code"] = "fail_closed"
                    resp["winner"] = None
                    resp["terminal_reward"] = [0, 0]
                    emit(resp)
                else:
                    emit(terminal_response(req["request_id"], req["episode_id"], expected_step))
            elif scenario == "provenance_drift":
                resp = decision_response(req["request_id"], req["episode_id"], req["expected_step"] + 1)
                resp["provenance"]["card_db_hash"] += 1
                emit(resp)
            elif scenario == "deck_hash_drift":
                resp = decision_response(req["request_id"], req["episode_id"], req["expected_step"] + 1)
                resp["deck_hashes"][0] += 1
                emit(resp)
            elif scenario == "invalid_terminal":
                resp = terminal_response(req["request_id"], req["episode_id"], req["expected_step"] + 1)
                resp["winner"] = "p1"
                emit(resp)
            elif scenario == "halted_terminal":
                resp = terminal_response(req["request_id"], req["episode_id"], req["expected_step"] + 1)
                resp["terminal_outcome"] = "halted"
                resp["terminal_classification"] = "halted"
                resp["terminal_code"] = "fail_closed"
                resp["winner"] = None
                resp["terminal_reward"] = [0, 0]
                emit(resp)
            elif scenario == "truncated_terminal":
                resp = terminal_response(req["request_id"], req["episode_id"], req["expected_step"] + 1)
                resp["terminal_outcome"] = "truncated"
                resp["terminal_classification"] = "truncated"
                resp["terminal_code"] = "decision_cap"
                resp["winner"] = None
                resp["terminal_reward"] = [0, 0]
                emit(resp)
            elif scenario == "terminal_jump":
                emit(terminal_response(req["request_id"], req["episode_id"], req["expected_step"] + 2))
            else:
                emit(terminal_response(req["request_id"], req["episode_id"], req["expected_step"] + 1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
