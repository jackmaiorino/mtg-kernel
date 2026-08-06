#!/usr/bin/env python3
"""Bind outcome-free policy identities for the next population refresh."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from build_initial_refresh import (
    PROGRAM_COMMIT,
    PROGRAM_DOCUMENT_SHA256,
    RETEST_MANIFEST_SHA256,
    slot,
)


LINEAGE_SEEDS = (970001, 970002, 970003)
ANCHOR_ROOT_KEYS = ("anchor_0", "anchor_1")


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_new_canonical(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()


def prior_weight(previous: dict, role: str) -> int:
    matches = [item for item in previous["slots"] if item["role"] == role]
    if len(matches) != 1:
        raise ValueError(f"previous refresh does not contain exactly one {role}")
    return int(matches[0]["weight_units"])


def root_for_seed(args: argparse.Namespace, seed: int) -> Path:
    mapping = {
        920012: args.anchor_0,
        920005: args.anchor_1,
        970001: args.seed_970001,
        970002: args.seed_970002,
        970003: args.seed_970003,
    }
    try:
        return mapping[seed]
    except KeyError as error:
        raise ValueError(f"no Store root was supplied for seed {seed}") from error


def bind(
    args: argparse.Namespace,
    previous: dict,
    refresh_index: int,
    global_generation: int,
) -> list[dict]:
    current_0_seed = LINEAGE_SEEDS[refresh_index % 3]
    current_1_seed = LINEAGE_SEEDS[(refresh_index + 1) % 3]
    historical_seed = LINEAGE_SEEDS[(refresh_index + 2) % 3]
    records = [
        slot(args.anchor_0, 920012, 384, "anchor-0", "policy", global_generation),
        slot(args.anchor_1, 920005, 512, "anchor-1", "policy", global_generation),
        slot(
            root_for_seed(args, historical_seed),
            historical_seed,
            global_generation - 256,
            "historical-0",
            "policy",
            global_generation,
        ),
        slot(
            root_for_seed(args, historical_seed),
            historical_seed,
            global_generation - 384,
            "historical-1",
            "policy",
            global_generation,
        ),
        slot(
            root_for_seed(args, current_0_seed),
            current_0_seed,
            global_generation,
            "current-0",
            "policy",
            global_generation,
        ),
        slot(
            root_for_seed(args, current_1_seed),
            current_1_seed,
            global_generation,
            "current-1",
            "policy",
            global_generation,
        ),
    ]

    program_update = refresh_index * 128
    if program_update % 256 == 0:
        raise ValueError(
            "this boundary requires a new exploiter build or explicit fallback selection"
        )
    for index, role in ((6, "exploiter-0"), (7, "exploiter-1")):
        prior = previous["slots"][index]
        records.append(
            slot(
                root_for_seed(args, int(prior["source_base_seed"])),
                int(prior["source_base_seed"]),
                int(prior["source_generation"]),
                role,
                str(prior["occupant_class"]),
                global_generation,
            )
        )

    models = {record["model_parameter_sha256"] for record in records}
    if len(models) != 8:
        raise ValueError("refresh bindings must contain eight unique model hashes")
    for record in records:
        weight = record.pop("weight_units")
        if weight != 125000:
            raise AssertionError("slot helper default weight changed")
        record["prior_weight_units"] = prior_weight(previous, record["role"])
    return records


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous-refresh", required=True, type=Path)
    parser.add_argument("--anchor-0", required=True, type=Path)
    parser.add_argument("--anchor-1", required=True, type=Path)
    parser.add_argument("--seed-970001", required=True, type=Path)
    parser.add_argument("--seed-970002", required=True, type=Path)
    parser.add_argument("--seed-970003", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    previous = load_json(args.previous_refresh)
    previous_index = int(previous["refresh_index"])
    refresh_index = previous_index + 1
    global_generation = 512 + refresh_index * 128
    if previous["global_generation"] != global_generation - 128:
        raise ValueError("previous refresh generation is not contiguous")
    records = bind(args, previous, refresh_index, global_generation)
    document = {
        "schema": "scaled-selfplay-population-slot-bindings/v1",
        "program_commit": PROGRAM_COMMIT,
        "program_document_sha256": PROGRAM_DOCUMENT_SHA256,
        "retest_manifest_sha256": RETEST_MANIFEST_SHA256,
        "refresh_index": refresh_index,
        "program_update": refresh_index * 128,
        "global_generation": global_generation,
        "previous_manifest_sha256": sha256_file(args.previous_refresh),
        "selection_uses_terminal_outcomes": False,
        "slots": records,
    }
    write_new_canonical(args.output, document)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
