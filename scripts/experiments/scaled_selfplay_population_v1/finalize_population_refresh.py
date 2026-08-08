#!/usr/bin/env python3
"""Apply the declared multiplicative-weights update and seal a refresh."""

from __future__ import annotations

import argparse
import hashlib
import json
from decimal import Decimal, getcontext
from pathlib import Path


ETA = Decimal("0.10")
TOTAL_UNITS = 1_000_000
ROLE_FLOOR = Decimal("0.20")
POLICY_CAP = Decimal("0.25")
ROLE_PAIRS = ((0, 1), (2, 3), (4, 5), (6, 7))


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_bytes(value: dict) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def write_new(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(canonical_bytes(value))
        stream.flush()


def normalize(weights: list[Decimal]) -> list[Decimal]:
    total = sum(weights)
    if total <= 0:
        raise ValueError("weight total is not positive")
    return [weight / total for weight in weights]


def constraints_hold(weights: list[Decimal]) -> bool:
    return all(weight <= POLICY_CAP for weight in weights) and all(
        weights[left] + weights[right] >= ROLE_FLOOR for left, right in ROLE_PAIRS
    )


def project_constraints(weights: list[Decimal]) -> tuple[list[Decimal], bool]:
    projected = normalize(weights)
    clipping_active = not constraints_hold(projected)
    tolerance = Decimal("1e-45")
    for _ in range(100):
        capped = [index for index, value in enumerate(projected) if value > POLICY_CAP]
        if capped:
            free = [index for index in range(8) if index not in capped]
            remaining = Decimal(1) - POLICY_CAP * len(capped)
            free_total = sum(projected[index] for index in free)
            if free_total <= 0:
                raise ValueError("policy-cap projection has no free mass")
            projected = [
                POLICY_CAP
                if index in capped
                else projected[index] * remaining / free_total
                for index in range(8)
            ]
        deficient = [
            pair
            for pair in ROLE_PAIRS
            if projected[pair[0]] + projected[pair[1]] < ROLE_FLOOR
        ]
        if deficient:
            deficient_indexes = {index for pair in deficient for index in pair}
            for left, right in deficient:
                role_total = projected[left] + projected[right]
                if role_total <= 0:
                    raise ValueError("role-floor projection has an empty role")
                projected[left] = projected[left] * ROLE_FLOOR / role_total
                projected[right] = projected[right] * ROLE_FLOOR / role_total
            free = [index for index in range(8) if index not in deficient_indexes]
            remaining = Decimal(1) - ROLE_FLOOR * len(deficient)
            free_total = sum(projected[index] for index in free)
            if free_total <= 0:
                raise ValueError("role-floor projection has no free mass")
            for index in free:
                projected[index] = projected[index] * remaining / free_total
        projected = normalize(projected)
        if constraints_hold(projected) and abs(sum(projected) - Decimal(1)) <= tolerance:
            return projected, clipping_active
    raise ValueError("weight projection did not converge")


def integerize(weights: list[Decimal]) -> list[int]:
    exact = [weight * TOTAL_UNITS for weight in weights]
    units = [int(value) for value in exact]
    remaining = TOTAL_UNITS - sum(units)
    order = sorted(range(8), key=lambda index: (-(exact[index] - units[index]), index))
    for index in order:
        if remaining == 0:
            break
        if units[index] < 250_000:
            units[index] += 1
            remaining -= 1
    if remaining:
        raise ValueError("integer policy caps leave undistributed units")
    for left, right in ROLE_PAIRS:
        while units[left] + units[right] < 200_000:
            recipient = left if exact[left] - units[left] >= exact[right] - units[right] else right
            donors = [
                index
                for index in range(8)
                if index not in (left, right)
                and units[index] > 0
                and units[(index // 2) * 2] + units[(index // 2) * 2 + 1] > 200_000
            ]
            if not donors or units[recipient] >= 250_000:
                raise ValueError("integer role-floor repair is infeasible")
            donor = max(donors, key=lambda index: (units[index] - exact[index], -index))
            units[donor] -= 1
            units[recipient] += 1
    if (
        sum(units) != TOTAL_UNITS
        or any(unit <= 0 or unit > 250_000 for unit in units)
        or any(units[left] + units[right] < 200_000 for left, right in ROLE_PAIRS)
    ):
        raise ValueError("integerized weights violate population constraints")
    return units


def main() -> int:
    getcontext().prec = 60
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous-refresh", required=True, type=Path)
    parser.add_argument("--bindings", required=True, type=Path)
    parser.add_argument("--payoff-panel", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--update-record", required=True, type=Path)
    args = parser.parse_args()

    previous = load_json(args.previous_refresh)
    bindings = load_json(args.bindings)
    panel = load_json(args.payoff_panel)
    binding_hash = sha256_file(args.bindings)
    if (
        panel.get("schema") != "scaled-selfplay-population-payoff-panel/v1"
        or panel["bindings_sha256"] != binding_hash
        or panel["global_generation"] != bindings["global_generation"]
        or panel["refresh_index"] != bindings["refresh_index"]
        or panel["matchup_count"] != 28
        or panel["pair_count_per_matchup"] != 512
        or panel["total_game_count"] != 28672
        or panel["terminal_reward_only"] is not True
    ):
        raise ValueError("payoff panel does not bind the refresh inputs")
    if (
        bindings["previous_manifest_sha256"] != sha256_file(args.previous_refresh)
        or previous["refresh_index"] + 1 != bindings["refresh_index"]
    ):
        raise ValueError("refresh chain is not contiguous")

    inputs_by_slot = {item["slot_index"]: item for item in panel["policy_inputs"]}
    raw = []
    calculations = []
    for slot in bindings["slots"]:
        index = int(slot["slot_index"])
        payoff = inputs_by_slot[index]
        if (
            payoff["model_parameter_sha256"] != slot["model_parameter_sha256"]
            or payoff["game_count"] != 7168
        ):
            raise ValueError(f"payoff input identity mismatch for slot {index}")
        normalized = Decimal(payoff["terminal_order_sum"]) / Decimal(payoff["game_count"])
        prior = Decimal(slot["prior_weight_units"]) / Decimal(TOTAL_UNITS)
        factor = (ETA * normalized).exp()
        value = prior * factor
        raw.append(value)
        calculations.append(
            {
                "slot_index": index,
                "role": slot["role"],
                "prior_weight_units": slot["prior_weight_units"],
                "terminal_order_sum": payoff["terminal_order_sum"],
                "game_count": payoff["game_count"],
                "normalized_terminal_order_payoff_decimal": str(normalized),
                "exp_eta_payoff_decimal": str(factor),
                "unnormalized_weight_decimal": str(value),
            }
        )
    projected, clipping_active = project_constraints(raw)
    units = integerize(projected)
    final_slots = []
    for slot, calculation, weight, unit in zip(
        bindings["slots"], calculations, projected, units, strict=True
    ):
        result = {key: value for key, value in slot.items() if key != "prior_weight_units"}
        result["weight_units"] = unit
        final_slots.append(result)
        calculation["projected_weight_decimal"] = str(weight)
        calculation["final_weight_units"] = unit

    refresh = {
        "schema": "mtg-kernel-scaled-selfplay-population-refresh/v1",
        "program_commit": bindings["program_commit"],
        "program_document_sha256": bindings["program_document_sha256"],
        "retest_manifest_sha256": bindings["retest_manifest_sha256"],
        "refresh_index": bindings["refresh_index"],
        "program_update": bindings["program_update"],
        "global_generation": bindings["global_generation"],
        "availability_generation": bindings["global_generation"],
        "weight_total_units": TOTAL_UNITS,
        "previous_manifest_sha256": sha256_file(args.previous_refresh),
        "payoff_panel_sha256": sha256_file(args.payoff_panel),
        "slots": final_slots,
    }
    update_record = {
        "schema": "scaled-selfplay-population-multiplicative-weights-update/v1",
        "eta_decimal": str(ETA),
        "role_floor_decimal": str(ROLE_FLOOR),
        "policy_cap_decimal": str(POLICY_CAP),
        "previous_manifest_sha256": sha256_file(args.previous_refresh),
        "bindings_sha256": binding_hash,
        "payoff_panel_sha256": sha256_file(args.payoff_panel),
        "clipping_active": clipping_active,
        "integer_rounding": "largest remainder, descending remainder then slot index",
        "calculations": calculations,
        "weight_total_units": sum(units),
    }
    write_new(args.output, refresh)
    update_record["refresh_manifest_sha256"] = sha256_file(args.output)
    write_new(args.update_record, update_record)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
