"""Read private actor-visible diagnostic JSONL. No model or engine execution."""
from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import struct

SCHEMA = "mtg-kernel-gameplay-decision-trace/v1"
TOTAL = 1 << 64
MASK = TOTAL - 1
GAMMA = 0x9E3779B97F4A7C15


def splitmix_first(seed: int) -> int:
    z = (seed + GAMMA) & MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def state_commitment(state: int) -> str:
    return hashlib.sha256(json.dumps({"state": state}, separators=(",", ":")).encode()).hexdigest()


def u64_commitment(value: int) -> str:
    return hashlib.sha256(struct.pack("<Q", value)).hexdigest()


def primary_rng_context(path: Path) -> dict[int, list[int]]:
    """Use private coordinator seeds without printing or copying them to the sidecar."""
    if path.stat().st_size > 64 * 1024**2:
        raise ValueError("primary journal exceeds reader bound")
    context = {}
    with path.open("rb") as stream:
        while line := stream.readline(2 * 1024**2 + 1):
            if len(line) > 2 * 1024**2 or not line.endswith(b"\n"):
                raise ValueError("invalid bounded primary journal")
            row = json.loads(line)
            if row["event"] == "game_start":
                payload = row["payload"]
                initial = payload["environment_seed"] ^ 0x50414952504F4C31
                context[payload["start"]["game_index"]] = [splitmix_first(initial), splitmix_first((initial + GAMMA) & MASK)]
    return context


def validate_decision(row: dict, rng_context: dict | None = None) -> None:
    actions = row["ordered_actions"]
    selected = row["selected_index"]
    count = len(actions)
    if not 0 < count <= 65536 or not 0 <= selected < count:
        raise ValueError("invalid menu or selected index")
    if [a["engine_index"] for a in actions] != list(range(count)):
        raise ValueError("engine order differs")
    if row["decision"]["legal_action_count"] != count:
        raise ValueError("decision menu count differs")
    behavior = row["behavior"]
    masses = [int(n) for n in behavior["mass_numerators"]]
    if behavior["kind"] != "hamilton_q64" or behavior["selected_index"] != selected:
        raise ValueError("executed distribution differs")
    if len(masses) != count or any(n < 0 or n > TOTAL for n in masses) or sum(masses) != TOTAL:
        raise ValueError("invalid exact Q64 distribution")
    rng = row["rng"]
    expected_rng_keys = {"seat", "draw_ordinal", "state_commitment_encoding", "u64_commitment_encoding",
                         "before_sha256", "after_sha256", "sampler_seed_sha256", "inverse_cdf_draw_sha256"}
    if set(rng) != expected_rng_keys or rng["seat"] != row["actor"] or not 0 <= rng["draw_ordinal"] <= MASK:
        raise ValueError("invalid private-safe RNG cursor")
    if rng["state_commitment_encoding"] != "sha256-compact-json-SplitMix64/v1" or rng["u64_commitment_encoding"] != "sha256-u64le/v1":
        raise ValueError("RNG commitment encoding differs")
    for key in ("before_sha256", "after_sha256", "sampler_seed_sha256", "inverse_cdf_draw_sha256"):
        if len(rng[key]) != 64 or any(c not in "0123456789abcdef" for c in rng[key]):
            raise ValueError("invalid RNG commitment")
    if rng_context is not None:
        seat = {"p0": 0, "p1": 1}[row["actor"]]
        initial = rng_context[row["decision"]["episode_id"]][seat]
        before = (initial + rng["draw_ordinal"] * GAMMA) & MASK
        seed = splitmix_first(before)
        draw = splitmix_first(seed)
        if (rng["before_sha256"], rng["after_sha256"], rng["sampler_seed_sha256"], rng["inverse_cdf_draw_sha256"]) != (
                state_commitment(before), state_commitment((before + GAMMA) & MASK), u64_commitment(seed), u64_commitment(draw)):
            raise ValueError("seat RNG cursor or sampler draw differs")
        cumulative = 0
        sampled = None
        for i, mass in enumerate(masses):
            cumulative += mass
            if draw < cumulative:
                sampled = i
                break
        if sampled != selected:
            raise ValueError("selected index differs from executed inverse CDF")
    binding = row["bound_engine_action"]
    if binding["engine_index"] != selected or binding["semantic"] != actions[selected]["semantic"]:
        raise ValueError("bound engine action differs")
    if (binding["episode_id"], binding["step"]) != (row["decision"]["episode_id"], row["decision"]["step"]):
        raise ValueError("execution decision binding differs")
    if row["transition"]["actor"] != row["actor"] or row["transition"]["observation"]["acting_player"] != row["actor"]:
        raise ValueError("transition exposes another actor")
    if len(row["logit_bits"]) != count or len(row["logits"]) != count:
        raise ValueError("logit menu count differs")
    for logit, bits in zip(row["logits"], row["logit_bits"]):
        if struct.unpack("<I", struct.pack("<f", logit))[0] != bits:
            raise ValueError("logit bits differ")
    encoded = json.dumps(row["encoded_input"], ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    if hashlib.sha256(encoded).hexdigest() != row["encoded_input_sha256"]:
        raise ValueError("encoded input hash differs")


def read_trace(path: Path, allow_incomplete: bool = False, rng_context: dict | None = None) -> tuple[dict, list[dict], dict]:
    if path.stat().st_size > 256 * 1024 * 1024:
        raise ValueError("sidecar exceeds reader byte bound")
    header = footer = None
    decisions = []
    previous = {"p0": None, "p1": None}
    with path.open("rb") as stream:
        while line := stream.readline(8 * 1024 * 1024 + 1):
            if len(line) > 8 * 1024 * 1024 or not line.endswith(b"\n"):
                raise ValueError("incomplete or oversized JSONL record")
            row = json.loads(line)
            if row.get("schema") != SCHEMA or footer is not None:
                raise ValueError("invalid schema or data after footer")
            if row["kind"] == "header" and header is None and not decisions:
                header = row
            elif row["kind"] == "decision" and header is not None:
                validate_decision(row, rng_context)
                if row["trace_index"] != len(decisions) or row["run_id"] != header["run_id"]:
                    raise ValueError("trace decision identity differs")
                if row["history"]["previous_actor_trace_index"] != previous[row["actor"]]:
                    raise ValueError("actor history link differs")
                previous[row["actor"]] = len(decisions)
                decisions.append(row)
                if len(decisions) > 100000:
                    raise ValueError("reader decision bound")
            elif row["kind"] == "footer" and header is not None:
                footer = row
            else:
                raise ValueError("invalid record order")
    if header is None or footer is None or footer["written"] != len(decisions):
        raise ValueError("missing header/footer or count mismatch")
    if not footer["complete"] and not allow_incomplete:
        raise ValueError(f"incomplete diagnostic capture: {footer['stop_reason']}")
    return header, decisions, footer


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--step", type=int)
    parser.add_argument("--episode", type=int)
    parser.add_argument("--index", type=int, help="trace-local decision index")
    parser.add_argument("--json", action="store_true", help="print complete matching records, including tensor bits")
    parser.add_argument("--allow-incomplete", action="store_true")
    parser.add_argument("--primary-journal", type=Path, help="private human V2 journal for RNG reproduction; seeds are never printed")
    args = parser.parse_args()
    context = primary_rng_context(args.primary_journal) if args.primary_journal else None
    header, decisions, footer = read_trace(args.trace, args.allow_incomplete, context)
    print(f"{len(decisions)} decisions; complete={footer['complete']}; runtime={header['identity']['runtime']['executable_sha256']}")
    print("RNG reproduced and verified" if context is not None else "RNG commitments only; supply --primary-journal to reproduce")
    for row in decisions:
        d = row["decision"]
        if args.step is not None and d["step"] != args.step or args.episode is not None and d["episode_id"] != args.episode or args.index is not None and row["trace_index"] != args.index:
            continue
        if args.json:
            print(json.dumps(row, ensure_ascii=False, sort_keys=True))
        else:
            print(f"decision {row['trace_index']}: episode={d['episode_id']} step={d['step']} actor={row['actor']} applied={row['transition']['applied']}")
            print("index\tlogit\texact executed probability\tlabel")
            for a, logit, mass in zip(row["ordered_actions"], row["logits"], row["behavior"]["mass_numerators"]):
                index = a["engine_index"]
                probability = Fraction(int(mass), TOTAL)
                print(f"{index}{'*' if index == row['selected_index'] else ''}\t{logit:.9g}\t{probability} ({float(probability):.9g})\t{a['label']}")


if __name__ == "__main__":
    main()
